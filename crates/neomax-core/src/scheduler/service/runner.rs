use std::sync::Arc;

use super::super::persistence::PlanTransition;
use super::super::runtime::{
    Clock, DispatchError, DispatchReceipt, DispatchRequest, DispatchResult, WorkerOutcome,
    WorkerRunner,
};
use super::events::event;
use super::ports::PersistencePort;
use crate::Result;

pub struct PersistentRunner<R, P, C> {
    inner: R,
    persistence: Arc<P>,
    clock: C,
    plan_id: String,
}

impl<R, P, C> PersistentRunner<R, P, C> {
    pub fn new(inner: R, persistence: Arc<P>, clock: C, plan_id: impl Into<String>) -> Self {
        Self {
            inner,
            persistence,
            clock,
            plan_id: plan_id.into(),
        }
    }
}

impl<R, P, C> WorkerRunner for PersistentRunner<R, P, C>
where
    R: WorkerRunner,
    P: PersistencePort,
    C: Clock + Clone,
{
    fn dispatch(&mut self, request: DispatchRequest) -> Result<DispatchReceipt> {
        self.dispatch_classified(request)
            .map_err(DispatchError::into_error)
    }

    fn dispatch_classified(&mut self, request: DispatchRequest) -> DispatchResult<DispatchReceipt> {
        self.persistence
            .append_event(
                &event(
                    &request.plan_id,
                    "worker_dispatch_requested",
                    self.clock.now(),
                    Some(&request.part_id),
                    None,
                )
                .map_err(|error| DispatchError::terminal(error.to_string()))?,
            )
            .map_err(|error| DispatchError::terminal(error.to_string()))?;
        let result = self.inner.dispatch_classified(request.clone());
        match result {
            Ok(receipt) => {
                let persisted = self.persistence.transition(
                    &request.plan_id,
                    PlanTransition::PartRunning {
                        part_id: request.part_id.clone(),
                        run_id: receipt.run_id.clone(),
                        branch: receipt.branch.clone(),
                        profile: receipt.profile.clone(),
                        at: receipt.launched_at,
                    },
                );
                if let Err(error) = persisted {
                    let _ = self.inner.cancel(&receipt.run_id);
                    if let Ok(event) = event(
                        &request.plan_id,
                        "worker_launch_persist_failed",
                        self.clock.now(),
                        Some(&request.part_id),
                        Some(&error.to_string()),
                    ) {
                        let _ = self.persistence.append_event(&event);
                    }
                    return Err(DispatchError::terminal(error.to_string()));
                }
                let launch_event = event(
                    &request.plan_id,
                    "worker_launched",
                    self.clock.now(),
                    Some(&request.part_id),
                    None,
                )
                .map_err(|error| DispatchError::terminal(error.to_string()))?;
                if let Err(error) = self.persistence.append_event(&launch_event) {
                    let _ = self.inner.cancel(&receipt.run_id);
                    let _ = self.persistence.transition(
                        &request.plan_id,
                        PlanTransition::PartFailed {
                            part_id: request.part_id.clone(),
                            error: error.to_string(),
                            at: self.clock.now(),
                        },
                    );
                    return Err(DispatchError::terminal(error.to_string()));
                }
                Ok(receipt)
            }
            Err(error @ DispatchError::Deferred { .. }) => {
                let reason = error.reason().to_owned();
                let deferred_event = event(
                    &request.plan_id,
                    "worker_dispatch_deferred",
                    self.clock.now(),
                    Some(&request.part_id),
                    Some(&reason),
                )
                .map_err(|event_error| DispatchError::terminal(event_error.to_string()))?;
                self.persistence
                    .append_event(&deferred_event)
                    .map_err(|persist_error| DispatchError::terminal(persist_error.to_string()))?;
                Err(error)
            }
            Err(error) => {
                let reason = error.reason().to_owned();
                let persist_error = self.persistence.transition(
                    &request.plan_id,
                    PlanTransition::PartFailed {
                        part_id: request.part_id.clone(),
                        error: reason.clone(),
                        at: self.clock.now(),
                    },
                );
                let event_result = event(
                    &request.plan_id,
                    "worker_dispatch_failed",
                    self.clock.now(),
                    Some(&request.part_id),
                    Some(&reason),
                )
                .and_then(|value| self.persistence.append_event(&value));
                if let Err(persist_error) = persist_error {
                    return Err(DispatchError::terminal(format!(
                        "worker dispatch failed: {reason}; failed to persist failure: {persist_error}"
                    )));
                }
                if let Err(event_error) = event_result {
                    return Err(DispatchError::terminal(format!(
                        "worker dispatch failed: {reason}; failed to persist failure event: {event_error}"
                    )));
                }
                Err(error)
            }
        }
    }

    fn poll(&mut self, run_id: &str) -> Result<Option<WorkerOutcome>> {
        self.persistence.append_event(&event(
            &self.plan_id,
            "worker_poll_requested",
            self.clock.now(),
            None,
            None,
        )?)?;
        let result = self.inner.poll(run_id)?;
        let name = if result.is_some() {
            "worker_outcome"
        } else {
            "worker_poll_empty"
        };
        self.persistence.append_event(&event(
            &self.plan_id,
            name,
            self.clock.now(),
            None,
            None,
        )?)?;
        Ok(result)
    }

    fn cancel(&mut self, run_id: &str) -> Result<()> {
        self.persistence.append_event(&event(
            &self.plan_id,
            "worker_cancel_requested",
            self.clock.now(),
            None,
            None,
        )?)?;
        let result = self.inner.cancel(run_id);
        if result.is_ok() {
            self.persistence.append_event(&event(
                &self.plan_id,
                "worker_cancelled",
                self.clock.now(),
                None,
                None,
            )?)?;
        }
        result
    }
}
