use std::thread;

use super::super::super::runtime::{
    DispatchError, DispatchReceipt, DispatchRequest, DispatchResult, WorkerOutcome,
};
use super::super::execution::{lock_error, outcome_error};
use super::{Job, ProviderExecution};
use crate::concurrency::dispatch::AdmissionRequest;
use crate::runs::{RunStatus, RunStore};
use crate::Error;
use crate::Result;

impl ProviderExecution {
    pub(super) fn dispatch_request(&self, request: DispatchRequest) -> Result<DispatchReceipt> {
        self.dispatch_request_classified(request)
            .map_err(DispatchError::into_error)
    }

    pub(super) fn dispatch_request_classified(
        &self,
        request: DispatchRequest,
    ) -> DispatchResult<DispatchReceipt> {
        {
            let jobs = self.inner.jobs.lock().map_err(|_| {
                DispatchError::terminal(lock_error("provider execution jobs").to_string())
            })?;
            if jobs.contains_key(&request.run_id) {
                return Err(DispatchError::terminal(format!(
                    "scheduler worker {} is already running",
                    request.run_id
                )));
            }
        }
        self.inner
            .admission
            .ensure_reserved(AdmissionRequest::new(
                request.run_id.clone(),
                request.plan_id.clone(),
                Some(request.engine),
            ))
            .map_err(classify_admission_error)?;
        let run = match self.new_run_classified(&request) {
            Ok(run) => run,
            Err(error) => {
                let _ = self.inner.admission.release(&request.run_id);
                return Err(error);
            }
        };
        if let Err(error) = self.inner.admission.bind(
            &request.run_id,
            request.engine,
            run.profile.to_string_lossy().into_owned(),
            request.run_id.clone(),
        ) {
            let _ = self.inner.admission.release(&request.run_id);
            return Err(classify_admission_error(error));
        }
        let internal_run_id = run.id.clone();
        let runs = RunStore::new(self.inner.paths.runs.clone());
        if let Err(error) = runs.create(&run) {
            let _ = self.inner.admission.release(&request.run_id);
            return Err(DispatchError::terminal(error.to_string()));
        }
        let scheduler_run_id = request.run_id.clone();
        let launch = DispatchReceipt {
            run_id: scheduler_run_id.clone(),
            branch: request.branch.clone(),
            profile: Some(run.profile.to_string_lossy().into_owned()),
            launched_at: self.inner.clock.now().timestamp(),
        };
        let execution = self.clone();
        let thread_name = format!("neomax-worker-{}", request.part_id);
        let handle = match thread::Builder::new()
            .name(thread_name)
            .spawn(move || execution.execute_run(&scheduler_run_id, run))
        {
            Ok(handle) => handle,
            Err(error) => {
                let message = format!("could not start worker thread: {error}");
                let _ = runs.update(&internal_run_id, |record| {
                    record.status = RunStatus::Error;
                    record.error_detail = Some(message.clone());
                    record.ended = Some(self.inner.clock.now().timestamp());
                    Ok(())
                });
                let _ = self.inner.admission.release(&request.run_id);
                return Err(DispatchError::terminal(message));
            }
        };
        let mut jobs = match self.inner.jobs.lock() {
            Ok(jobs) => jobs,
            Err(_) => {
                return Err(DispatchError::terminal(
                    lock_error("provider execution jobs").to_string(),
                ));
            }
        };
        if jobs.contains_key(&launch.run_id) {
            drop(jobs);
            let _ = RunStore::new(self.inner.paths.runs.clone()).update(&internal_run_id, |run| {
                run.killed = true;
                run.status = RunStatus::Aborted;
                run.ended = Some(self.inner.clock.now().timestamp());
                Ok(())
            });
            let _ = handle.join();
            let _ = self.inner.admission.release(&request.run_id);
            return Err(DispatchError::terminal(format!(
                "scheduler worker {} is already running",
                launch.run_id
            )));
        }
        jobs.insert(
            launch.run_id.clone(),
            Job {
                internal_run_id,
                handle,
            },
        );
        Ok(launch)
    }

    pub(super) fn poll_request(&self, run_id: &str) -> Result<Option<WorkerOutcome>> {
        let finished = {
            let jobs = self
                .inner
                .jobs
                .lock()
                .map_err(|_| lock_error("provider execution jobs"))?;
            jobs.get(run_id).is_some_and(|job| job.handle.is_finished())
        };
        if !finished {
            return Ok(None);
        }
        let job = self
            .inner
            .jobs
            .lock()
            .map_err(|_| lock_error("provider execution jobs"))?
            .remove(run_id);
        let Some(job) = job else {
            return Ok(None);
        };
        let Job {
            internal_run_id,
            handle,
        } = job;
        match handle.join() {
            Ok(Ok(outcome)) => {
                let _ = self.inner.admission.release(run_id);
                Ok(Some(outcome))
            }
            Ok(Err(error)) => {
                self.mark_error(&internal_run_id, error.to_string());
                let _ = self.inner.admission.release(run_id);
                Ok(Some(outcome_error(run_id, error)))
            }
            Err(_) => {
                self.mark_error(&internal_run_id, "worker thread panicked");
                let _ = self.inner.admission.release(run_id);
                Ok(Some(outcome_error(run_id, "worker thread panicked")))
            }
        }
    }

    pub(super) fn cancel_request(&self, run_id: &str) -> Result<()> {
        let Some(internal_id) = self.find_internal_run(run_id)? else {
            return Ok(());
        };
        let runs = RunStore::new(self.inner.paths.runs.clone());
        let now = self.inner.clock.now().timestamp();
        let _ = runs.update(&internal_id, |run| {
            if !run.status.is_terminal() {
                run.killed = true;
                run.status = RunStatus::Aborted;
                run.interrupt_signal = None;
                run.ended = Some(now);
            }
            Ok(())
        })?;
        Ok(())
    }

    fn find_internal_run(&self, scheduler_run_id: &str) -> Result<Option<String>> {
        if let Some(job) = self
            .inner
            .jobs
            .lock()
            .map_err(|_| lock_error("provider execution jobs"))?
            .get(scheduler_run_id)
        {
            return Ok(Some(job.internal_run_id.clone()));
        }
        let runs = RunStore::new(self.inner.paths.runs.clone());
        Ok(runs
            .all()?
            .into_iter()
            .filter(|run| {
                run.extra
                    .get("scheduler_run_id")
                    .and_then(serde_json::Value::as_str)
                    == Some(scheduler_run_id)
            })
            .max_by_key(|run| {
                (
                    run.started,
                    run.extra
                        .get("scheduler_attempt")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or_default(),
                )
            })
            .map(|run| run.id))
    }

    fn mark_error(&self, internal_run_id: &str, error: impl Into<String>) {
        let error = error.into();
        let runs = RunStore::new(self.inner.paths.runs.clone());
        let _ = runs.update(internal_run_id, |run| {
            if !run.status.is_terminal() {
                run.status = RunStatus::Error;
                run.error_detail = Some(error.clone());
                run.ended = Some(self.inner.clock.now().timestamp());
            }
            Ok(())
        });
    }
}

fn classify_admission_error(error: Error) -> DispatchError {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    if lower.contains("dispatch cap")
        || lower.contains("lane cap")
        || lower.contains("session cap")
        || lower.contains("capacity")
    {
        DispatchError::deferred(message)
    } else {
        DispatchError::terminal(message)
    }
}
