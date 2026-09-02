use std::collections::BTreeMap;

use super::super::super::{Plan, PlanState};
use super::super::admission::AdmissionController;
use super::super::clock::Clock;
use super::super::dispatch::{
    DispatchPlanner, DispatchRequest, RecoveredWorker, WorkerOutcome, WorkerRunner,
};
use super::dispatching::dispatch_ready;
use super::reconciliation::reconcile_completed;
use super::stalled::block_remaining;
use super::types::{RuntimeConfig, TickReport};
use super::validation::validate_state;
use crate::{Error, Result};

pub(super) enum ActiveWorker {
    Managed,
    Recovered(Box<dyn RecoveredWorker>),
}

pub(super) struct ActivePart {
    pub(super) request: DispatchRequest,
    pub(super) worker_run_id: String,
    pub(super) attempt: u32,
    pub(super) worker: ActiveWorker,
}

pub struct RuntimeCoordinator<R, A, C, P> {
    pub(super) plan: Plan,
    pub(super) state: PlanState,
    pub(super) graph: super::super::super::DependencyGraph,
    pub(super) runner: R,
    pub(super) admission: A,
    pub(super) clock: C,
    pub(super) planner: P,
    pub(super) config: RuntimeConfig,
    pub(super) active: BTreeMap<String, ActivePart>,
    pub(super) attempts: BTreeMap<String, u32>,
    pub(super) retry_after: BTreeMap<String, i64>,
    pub(super) stall_cycles: usize,
}

impl<R, A, C, P> RuntimeCoordinator<R, A, C, P>
where
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock,
    P: DispatchPlanner,
{
    pub fn new(
        plan: Plan,
        state: PlanState,
        runner: R,
        admission: A,
        clock: C,
        planner: P,
        config: RuntimeConfig,
    ) -> Result<Self> {
        config.validate()?;
        let graph = plan.graph()?;
        validate_state(&plan, &state)?;
        if plan.plan_id.is_none() {
            return Err(Error::InvalidArgument("scheduler plan has no id".into()));
        }
        Ok(Self {
            plan,
            state,
            graph,
            runner,
            admission,
            clock,
            planner,
            config,
            active: BTreeMap::new(),
            attempts: BTreeMap::new(),
            retry_after: BTreeMap::new(),
            stall_cycles: 0,
        })
    }

    pub fn state(&self) -> &PlanState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut PlanState {
        &mut self.state
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Cancels the parts owned by this coordinator and releases only the
    /// leases represented by those active requests. A failed cancellation
    /// stays active so a later idempotent interrupt can retry it.
    pub fn cancel_active(&mut self) -> Result<Vec<String>> {
        let active = self.active.keys().cloned().collect::<Vec<_>>();
        let mut cancelled = Vec::new();
        let mut errors = Vec::new();
        for part_id in active {
            let Some(mut part) = self.active.remove(&part_id) else {
                continue;
            };
            let run_id = part.worker_run_id.clone();
            let result = match &mut part.worker {
                ActiveWorker::Managed => self.runner.cancel(&run_id),
                ActiveWorker::Recovered(worker) => worker.cancel(),
            };
            match result {
                Ok(()) => {
                    if matches!(&part.worker, ActiveWorker::Managed) {
                        self.admission.release_after_cancel(&part.request);
                    } else {
                        self.admission.release(&part.request);
                    }
                    cancelled.push(part_id);
                }
                Err(error) => {
                    self.active.insert(part_id.clone(), part);
                    errors.push(format!("part {part_id}: {error}"));
                }
            }
        }
        if errors.is_empty() {
            Ok(cancelled)
        } else {
            Err(Error::Message(format!(
                "scheduler cancellation failed: {}",
                errors.join("; ")
            )))
        }
    }

    pub fn attempt(&self, part_id: &str) -> u32 {
        self.attempts.get(part_id).copied().unwrap_or_default()
    }

    pub(crate) fn admit_recovered(&mut self, request: &DispatchRequest) -> Result<()> {
        match self.admission.admit_recovered(request) {
            super::super::admission::AdmissionDecision::Admitted { .. } => Ok(()),
            super::super::admission::AdmissionDecision::AreaBusy { areas } => {
                Err(Error::Conflict(format!(
                    "recovered scheduler part {} could not reacquire areas {}",
                    request.part_id,
                    areas.join(", ")
                )))
            }
            super::super::admission::AdmissionDecision::CapacityExhausted { active, maximum } => {
                Err(Error::Conflict(format!(
                    "recovered scheduler part {} exceeds live capacity ({active}/{maximum})",
                    request.part_id
                )))
            }
        }
    }

    pub(crate) fn register_recovered(
        &mut self,
        part_id: String,
        request: DispatchRequest,
        attempt: u32,
        worker: Box<dyn RecoveredWorker>,
    ) -> Result<()> {
        if self.state.state(&part_id) != Some(super::super::super::PartState::Running) {
            return Err(Error::Conflict(format!(
                "recovered scheduler part {part_id} is not running"
            )));
        }
        if self.active.contains_key(&part_id) {
            return Err(Error::Conflict(format!(
                "scheduler part {part_id} is already active"
            )));
        }
        if request.run_id.is_empty() {
            return Err(Error::InvalidState {
                path: request.cwd,
                message: format!("recovered scheduler part {part_id} has no run id"),
            });
        }
        self.attempts.insert(part_id.clone(), attempt);
        self.active.insert(
            part_id,
            ActivePart {
                worker_run_id: request.run_id.clone(),
                request,
                attempt,
                worker: ActiveWorker::Recovered(worker),
            },
        );
        Ok(())
    }

    pub(crate) fn release_recovered(&mut self, request: &DispatchRequest) {
        self.admission.release(request);
    }

    pub(crate) fn poll_active(&mut self, part_id: &str) -> Result<Option<WorkerOutcome>> {
        let (recovered, worker_run_id) = {
            let Some(active) = self.active.get(part_id) else {
                return Ok(None);
            };
            (
                matches!(&active.worker, ActiveWorker::Recovered(_)),
                active.worker_run_id.clone(),
            )
        };
        if recovered {
            let active = self
                .active
                .get_mut(part_id)
                .expect("active part disappeared during recovery poll");
            if let ActiveWorker::Recovered(worker) = &mut active.worker {
                return worker.poll();
            }
        }
        self.runner.poll(&worker_run_id)
    }

    pub fn tick(&mut self) -> Result<TickReport> {
        let mut report = TickReport::empty();
        report.blocked = self.state.block_failed_dependencies(&self.graph);
        dispatch_ready(self, &mut report)?;
        reconcile_completed(self, &mut report)?;

        if !report.progressed()
            && self.active.is_empty()
            && !self.state.finished()
            && !self.waiting_for_retry()
        {
            self.stall_cycles = self.stall_cycles.saturating_add(1);
            if self.stall_cycles >= self.config.max_stall_cycles {
                block_remaining(self, &mut report)?;
                report.stalled = true;
            }
        } else {
            self.stall_cycles = 0;
        }
        report.finished = self.state.finished();
        Ok(report)
    }

    pub(super) fn waiting_for_retry(&self) -> bool {
        let now = self.clock.now();
        self.retry_after.values().any(|retry_at| *retry_at > now)
    }

    pub fn run_until_terminal(&mut self, max_ticks: usize) -> Result<PlanState> {
        if max_ticks == 0 {
            return Err(Error::InvalidArgument(
                "scheduler max_ticks must be positive".into(),
            ));
        }
        for _ in 0..max_ticks {
            if self.state.finished() {
                return Ok(self.state.clone());
            }
            self.tick()?;
        }
        if self.state.finished() {
            Ok(self.state.clone())
        } else {
            Err(Error::Conflict(format!(
                "scheduler plan did not reach a terminal state after {max_ticks} ticks"
            )))
        }
    }
}
