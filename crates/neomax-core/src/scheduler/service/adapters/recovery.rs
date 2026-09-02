use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::super::super::runtime::{DispatchRequest, RecoveredWorker, WorkerOutcome};
use super::super::ports::{RecoveryPort, RecoveryStatus};
use crate::io::process_group;
use crate::io::{read_file, LocalFileSource, ReadLimits};
use crate::runs::{worker_state, ProbeState, ProcessProbe, RunStatus, RunStore};
use crate::{Error, Result};

pub(crate) const MAX_RECOVERY_RUN_BYTES: usize = 4 * 1024 * 1024;
const RECOVERY_RUN_READ_TIMEOUT: Duration = Duration::from_secs(5);

pub struct CoordinatorRecovery<P> {
    runs: RunStore,
    runs_directory: PathBuf,
    probe: Arc<P>,
}

pub type SystemCoordinatorRecovery = CoordinatorRecovery<crate::runs::SystemProcessProbe>;

pub fn system_coordinator_recovery(
    runs_directory: impl Into<PathBuf>,
) -> SystemCoordinatorRecovery {
    CoordinatorRecovery::new(runs_directory, crate::runs::SystemProcessProbe)
}

impl<P> CoordinatorRecovery<P> {
    pub fn new(runs_directory: impl Into<PathBuf>, probe: P) -> Self {
        let runs_directory = runs_directory.into();
        Self {
            runs: RunStore::new(runs_directory.clone()),
            runs_directory,
            probe: Arc::new(probe),
        }
    }

    pub fn runs(&self) -> &RunStore {
        &self.runs
    }
}

impl<P> RecoveryPort for CoordinatorRecovery<P>
where
    P: ProcessProbe + 'static,
{
    fn inspect(
        &mut self,
        request: &DispatchRequest,
        execution: &crate::scheduler::PartExecution,
    ) -> Result<RecoveryStatus> {
        let expected_run_id = execution
            .run_id
            .as_deref()
            .ok_or_else(|| Error::InvalidState {
                path: request.cwd.clone(),
                message: "running part has no persisted worker run id".into(),
            })?;
        if expected_run_id != request.run_id {
            return Err(Error::Conflict(format!(
                "recovery run id {} does not match request {}",
                expected_run_id, request.run_id
            )));
        }
        let Some(run) = self.find_run(expected_run_id)? else {
            return Ok(RecoveryStatus::Failed(WorkerOutcome::Missing {
                run_id: request.run_id.clone(),
                error: Some("provider run record is missing".into()),
            }));
        };
        if run_is_live(&run, self.probe.as_ref(), &request.run_id)? {
            return Ok(RecoveryStatus::StillRunning);
        }
        if run.status == RunStatus::Running {
            return Ok(RecoveryStatus::Failed(WorkerOutcome::Interrupted {
                run_id: request.run_id.clone(),
                error: Some("provider worker is no longer running".into()),
            }));
        }
        Ok(status_outcome(&request.run_id, &run))
    }

    fn live_handle(
        &mut self,
        request: &DispatchRequest,
        execution: &crate::scheduler::PartExecution,
    ) -> Result<Option<Box<dyn RecoveredWorker>>> {
        let expected_run_id = execution
            .run_id
            .as_deref()
            .ok_or_else(|| Error::InvalidState {
                path: request.cwd.clone(),
                message: "running part has no persisted worker run id".into(),
            })?;
        if expected_run_id != request.run_id {
            return Err(Error::Conflict(format!(
                "recovery run id {} does not match request {}",
                expected_run_id, request.run_id
            )));
        }
        Ok(Some(Box::new(CoordinatorRecoveryHandle {
            runs: self.runs.clone(),
            runs_directory: self.runs_directory.clone(),
            probe: Arc::clone(&self.probe),
            request: request.clone(),
        })))
    }
}

struct CoordinatorRecoveryHandle<P> {
    runs: RunStore,
    runs_directory: PathBuf,
    probe: Arc<P>,
    request: DispatchRequest,
}

impl<P> CoordinatorRecoveryHandle<P>
where
    P: ProcessProbe,
{
    fn find_run(&self) -> Result<Option<crate::runs::RunRecord>> {
        find_run(&self.runs, &self.runs_directory, &self.request.run_id)
    }
}

impl<P> RecoveredWorker for CoordinatorRecoveryHandle<P>
where
    P: ProcessProbe,
{
    fn poll(&mut self) -> Result<Option<WorkerOutcome>> {
        let Some(run) = self.find_run()? else {
            return Ok(Some(WorkerOutcome::Missing {
                run_id: self.request.run_id.clone(),
                error: Some("provider run record is missing".into()),
            }));
        };
        if run_is_live(&run, self.probe.as_ref(), &self.request.run_id)? {
            return Ok(None);
        }
        if run.status == RunStatus::Running {
            return Ok(Some(WorkerOutcome::Interrupted {
                run_id: self.request.run_id.clone(),
                error: Some("provider worker is no longer running".into()),
            }));
        }
        Ok(Some(status_worker_outcome(&self.request.run_id, &run)))
    }

    fn cancel(&mut self) -> Result<()> {
        let Some(run) = self.find_run()? else {
            return Ok(());
        };
        let supervisor_pid = run.supervisor_pid;
        let supervisor = supervisor_pid.map_or(ProbeState::Dead, |pid| self.probe.pid_state(pid));
        let worker = worker_state(&run, self.probe.as_ref());
        let now = chrono::Utc::now().timestamp();
        self.runs.update(&run.id, |record| {
            if !record.status.is_terminal() {
                record.killed = true;
                record.status = RunStatus::Aborted;
                record.interrupt_signal = Some(15);
                record.acknowledged = Some(false);
                record.ended = Some(now);
            }
            Ok(())
        })?;

        let mut errors = Vec::new();
        match supervisor {
            ProbeState::Alive => {
                if let Some(pid) = supervisor_pid {
                    if let Err(error) = process_group::terminate_supervisor(pid) {
                        errors.push(format!("supervisor {pid}: {error}"));
                    }
                }
            }
            ProbeState::Unknown => errors.push(format!(
                "supervisor liveness is indeterminate for run {}",
                self.request.run_id
            )),
            ProbeState::Dead => {}
        }

        let worker_after_supervisor = run
            .worker_pid
            .map_or(worker, |pid| self.probe.worker_state(pid, run.engine));
        match worker_after_supervisor {
            ProbeState::Alive => {
                if let Some(pid) = run.worker_pid {
                    if let Err(error) = process_group::terminate_worker(pid) {
                        errors.push(format!("worker {pid}: {error}"));
                    }
                }
            }
            ProbeState::Unknown => errors.push(format!(
                "worker liveness is indeterminate for run {}",
                self.request.run_id
            )),
            ProbeState::Dead => {}
        }
        if let Some(pid) = supervisor_pid {
            match self.probe.pid_state(pid) {
                ProbeState::Alive => errors.push(format!(
                    "provider supervisor {pid} is still alive after cancellation"
                )),
                ProbeState::Unknown => errors.push(format!(
                    "provider supervisor liveness is indeterminate for run {}",
                    self.request.run_id
                )),
                ProbeState::Dead => {}
            }
        }
        if let Some(pid) = run.worker_pid {
            match self.probe.worker_state(pid, run.engine) {
                ProbeState::Alive => errors.push(format!(
                    "provider worker {pid} is still alive after cancellation"
                )),
                ProbeState::Unknown => errors.push(format!(
                    "provider worker liveness is indeterminate for run {}",
                    self.request.run_id
                )),
                ProbeState::Dead => {}
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Message(errors.join("; ")))
        }
    }
}

fn interruption_still_running<P>(run: &crate::runs::RunRecord, probe: &P) -> Result<bool>
where
    P: ProcessProbe,
{
    if !run.status.is_interruption() || run.is_acknowledged() {
        return Ok(false);
    }
    let supervisor = run
        .supervisor_pid
        .map_or(ProbeState::Dead, |pid| probe.pid_state(pid));
    let worker = worker_state(run, probe);
    process_is_live(supervisor, worker, &run.id)
}

impl<P> CoordinatorRecovery<P>
where
    P: ProcessProbe,
{
    fn find_run(&self, scheduler_run_id: &str) -> Result<Option<crate::runs::RunRecord>> {
        find_run(&self.runs, &self.runs_directory, scheduler_run_id)
    }
}

fn find_run(
    runs: &RunStore,
    runs_directory: &Path,
    scheduler_run_id: &str,
) -> Result<Option<crate::runs::RunRecord>> {
    let mut matches = runs
        .all()?
        .into_iter()
        .filter(|run| {
            run.id == scheduler_run_id
                || run
                    .extra
                    .get("scheduler_run_id")
                    .and_then(serde_json::Value::as_str)
                    == Some(scheduler_run_id)
        })
        .collect::<Vec<_>>();
    append_strict_prefix_matches(runs_directory, scheduler_run_id, &mut matches)?;
    matches.sort_by_key(|run| {
        (
            run.started,
            run.extra
                .get("scheduler_attempt")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default(),
        )
    });
    Ok(matches.pop())
}

fn append_strict_prefix_matches(
    runs_directory: &Path,
    scheduler_run_id: &str,
    matches: &mut Vec<crate::runs::RunRecord>,
) -> Result<()> {
    let prefix = format!("{scheduler_run_id}-attempt-");
    let entries = match fs::read_dir(runs_directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !stem.starts_with(&prefix) {
            continue;
        }
        let bytes = read_file(
            &LocalFileSource,
            &path,
            ReadLimits::new(MAX_RECOVERY_RUN_BYTES, RECOVERY_RUN_READ_TIMEOUT)?,
        )?;
        let run = serde_json::from_slice::<crate::runs::RunRecord>(&bytes).map_err(|error| {
            Error::InvalidState {
                path: path.clone(),
                message: format!("corrupt provider run record: {error}"),
            }
        })?;
        if run.id == stem && !matches.iter().any(|candidate| candidate.id == run.id) {
            matches.push(run);
        }
    }
    Ok(())
}

fn run_is_live<P>(
    run: &crate::runs::RunRecord,
    probe: &P,
    indeterminate_run_id: &str,
) -> Result<bool>
where
    P: ProcessProbe,
{
    if run.status == RunStatus::Running {
        let supervisor = run
            .supervisor_pid
            .map_or(ProbeState::Dead, |pid| probe.pid_state(pid));
        let worker = worker_state(run, probe);
        return process_is_live(supervisor, worker, indeterminate_run_id);
    }
    if run.status == RunStatus::Orphaned {
        match worker_state(run, probe) {
            ProbeState::Alive => return Ok(true),
            ProbeState::Unknown => {
                return Err(Error::Conflict(format!(
                    "process liveness is indeterminate for run {}",
                    indeterminate_run_id
                )));
            }
            ProbeState::Dead => return Ok(false),
        }
    }
    interruption_still_running(run, probe)
}

fn process_is_live(
    supervisor: ProbeState,
    worker: ProbeState,
    indeterminate_run_id: &str,
) -> Result<bool> {
    match (supervisor, worker) {
        (ProbeState::Alive, _) | (_, ProbeState::Alive) => Ok(true),
        (ProbeState::Unknown, _) | (_, ProbeState::Unknown) => Err(Error::Conflict(format!(
            "process liveness is indeterminate for run {}",
            indeterminate_run_id
        ))),
        (ProbeState::Dead, ProbeState::Dead) => Ok(false),
    }
}

fn status_outcome(scheduler_run_id: &str, run: &crate::runs::RunRecord) -> RecoveryStatus {
    let outcome = status_worker_outcome(scheduler_run_id, run);
    if matches!(outcome, WorkerOutcome::Completed { .. }) {
        RecoveryStatus::Completed(outcome)
    } else {
        RecoveryStatus::Failed(outcome)
    }
}

fn status_worker_outcome(scheduler_run_id: &str, run: &crate::runs::RunRecord) -> WorkerOutcome {
    match run.status {
        RunStatus::Done | RunStatus::Integrated => WorkerOutcome::Completed {
            run_id: scheduler_run_id.to_owned(),
        },
        RunStatus::Limit => WorkerOutcome::RateLimited {
            run_id: scheduler_run_id.to_owned(),
            retry_at: run.resets_at.map(|value| value as i64),
            error: run.error_detail.clone(),
        },
        RunStatus::Aborted | RunStatus::Interrupted => WorkerOutcome::Interrupted {
            run_id: scheduler_run_id.to_owned(),
            error: run.error_detail.clone(),
        },
        _ => WorkerOutcome::Failed {
            run_id: scheduler_run_id.to_owned(),
            error: run
                .error_detail
                .clone()
                .unwrap_or_else(|| format!("provider run finished with {}", run.status.as_str())),
        },
    }
}
