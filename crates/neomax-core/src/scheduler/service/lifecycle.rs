use super::super::persistence::{PlanStatus, PlanTransition};
use super::super::runtime::{
    AdmissionController, Clock, DispatchPlanner, TickReport, WorkerRunner,
};
use super::events::append;
use super::model::RunAllService;
use super::ports::{PersistencePort, RecoveryPort, RecoveryStatus, WorkspacePort};
use super::recovery::{rehydrate_running_parts, RecoveryReport};
use super::side_effects::DurableDispatchPlanner;
use crate::io::process_group;
use crate::{Error, Result};

impl<P, W, R, A, C> RunAllService<P, W, R, A, C>
where
    P: PersistencePort,
    W: WorkspacePort,
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock + Clone,
{
    pub fn tick(&mut self) -> Result<TickReport> {
        let plan_id = self.plan_id()?.to_owned();
        let status = self.persistence.load(&plan_id)?.status;
        if matches!(status, PlanStatus::Interrupted | PlanStatus::Killed) {
            let cancellation = self.coordinator.cancel_active();
            if cancellation.is_ok() {
                self.release_owned_supervisor(&plan_id)?;
            }
            cancellation?;
            return Err(Error::Conflict(format!(
                "scheduler plan {plan_id} is {status}",
                status = status_name(status)
            )));
        }
        self.ensure_supervisor()?;
        let report = self.coordinator.tick()?;
        if let Some(error) = self.admission_errors.take() {
            return Err(Error::Message(format!(
                "scheduler area admission durability failed: {error}"
            )));
        }
        self.persist_state()?;
        Ok(report)
    }

    pub fn run_until_terminal(&mut self, max_ticks: usize) -> Result<super::super::PlanState> {
        if max_ticks == 0 {
            return Err(Error::InvalidArgument(
                "scheduler max_ticks must be positive".into(),
            ));
        }
        for _ in 0..max_ticks {
            if self.coordinator.state().finished() {
                return Ok(self.coordinator.state().clone());
            }
            self.tick()?;
        }
        if self.coordinator.state().finished() {
            Ok(self.coordinator.state().clone())
        } else {
            Err(Error::Conflict(format!(
                "scheduler plan did not reach a terminal state after {max_ticks} ticks"
            )))
        }
    }

    pub fn recover<Q>(&mut self, recovery: &mut Q) -> Result<RecoveryReport>
    where
        Q: RecoveryPort,
    {
        self.ensure_supervisor()?;
        let plan_id =
            self.coordinator
                .plan()
                .plan_id
                .as_deref()
                .ok_or_else(|| Error::InvalidState {
                    path: "scheduler".into(),
                    message: "runtime plan has no id".into(),
                })?;
        let mut record = self.persistence.load(plan_id)?;
        if !matches!(record.status, PlanStatus::Interrupted | PlanStatus::Killed) {
            return Err(Error::Conflict(format!(
                "scheduler plan {plan_id} is not recoverable from {}",
                status_name(record.status)
            )));
        }
        append(
            self.persistence.as_ref(),
            plan_id,
            "plan_recovery_requested",
            self.coordinator_now(),
            None,
        )?;
        self.persistence.transition(
            plan_id,
            PlanTransition::Recover {
                at: self.coordinator_now(),
            },
        )?;
        append(
            self.persistence.as_ref(),
            plan_id,
            "plan_recovered",
            self.coordinator_now(),
            None,
        )?;
        record = self.persistence.load(plan_id)?;

        let planner = DurableDispatchPlanner::new(
            self.workspace.clone(),
            self.persistence.clone(),
            self.integration.clone(),
            self.clock.clone(),
        );
        *self.coordinator.state_mut() = record.state.clone();
        let report = rehydrate_running_parts(
            self.persistence.as_ref(),
            recovery,
            &planner,
            &mut self.coordinator,
            &mut record,
            self.clock.clone(),
        )?;
        self.persist_state()?;
        if let Some(error) = self.admission_errors.take() {
            return Err(Error::Message(format!(
                "scheduler area admission durability failed: {error}"
            )));
        }
        Ok(report)
    }

    pub fn interrupt(&mut self, error: Option<String>) -> Result<()> {
        let plan_id = self.plan_id()?.to_owned();
        let record = self.persistence.load(&plan_id)?;
        if !record.status.is_terminal() {
            append(
                self.persistence.as_ref(),
                &plan_id,
                "plan_interrupt_requested",
                self.coordinator_now(),
                None,
            )?;
            self.persistence.transition(
                &plan_id,
                PlanTransition::Interrupted {
                    error: error.clone(),
                    at: self.coordinator_now(),
                },
            )?;
            append(
                self.persistence.as_ref(),
                &plan_id,
                "plan_interrupted",
                self.coordinator_now(),
                None,
            )?;
        }
        let supervisor_stop = self.stop_supervisor(&record);
        let cancellation = self.coordinator.cancel_active();
        let mut errors = Vec::new();
        if let Err(error) = supervisor_stop {
            errors.push(format!("scheduler supervisor stop failed: {error}"));
        }
        match cancellation {
            Ok(_) => {
                if let Err(error) = self.release_owned_supervisor(&plan_id) {
                    errors.push(format!("scheduler supervisor release failed: {error}"));
                }
            }
            Err(error) => errors.push(error.to_string()),
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Message(errors.join("; ")))
        }
    }

    pub fn interrupt_with_recovery<Q>(
        &mut self,
        recovery: &mut Q,
        error: Option<String>,
    ) -> Result<()>
    where
        Q: RecoveryPort,
    {
        let interrupt_error = self.interrupt(error).err();
        let plan_id = self.plan_id()?.to_owned();
        let record = self.persistence.load(&plan_id)?;
        if let Some(error) = interrupt_error.as_ref() {
            if !record.status.is_terminal() {
                return Err(Error::Message(error.to_string()));
            }
        }
        let planner = DurableDispatchPlanner::new(
            self.workspace.clone(),
            self.persistence.clone(),
            self.integration.clone(),
            self.clock.clone(),
        );
        let running = record
            .state
            .states
            .iter()
            .filter_map(|(part_id, state)| {
                (*state == super::super::PartState::Running).then_some(part_id.clone())
            })
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        if let Some(error) = interrupt_error {
            errors.push(format!("plan interruption incomplete: {error}"));
        }
        for part_id in running {
            let Some(part) = record.plan.part(&part_id) else {
                errors.push(format!("scheduler part {part_id} is missing from the plan"));
                continue;
            };
            let Some(execution) = record.state.execution(&part_id).cloned() else {
                errors.push(format!("running scheduler part {part_id} has no execution"));
                continue;
            };
            let Some(run_id) = execution.run_id.clone() else {
                errors.push(format!("running scheduler part {part_id} has no run id"));
                continue;
            };
            let mut request = match planner.plan(&record.plan, part, 1) {
                Ok(request) => request,
                Err(error) => {
                    errors.push(format!("part {part_id}: {error}"));
                    continue;
                }
            };
            request.run_id = run_id.clone();
            let inspection = match recovery.inspect(&request, &execution) {
                Ok(status) => status,
                Err(error) => {
                    errors.push(format!("part {part_id}: {error}"));
                    continue;
                }
            };
            match inspection {
                RecoveryStatus::StillRunning => {}
                RecoveryStatus::Completed(outcome) | RecoveryStatus::Failed(outcome) => {
                    if outcome.run_id() != request.run_id {
                        errors.push(format!(
                            "part {part_id} recovery outcome run id {} does not match {}",
                            outcome.run_id(),
                            request.run_id
                        ));
                        continue;
                    }
                    self.coordinator.release_recovered(&request);
                    append(
                        self.persistence.as_ref(),
                        &plan_id,
                        "worker_interrupt_already_complete",
                        self.coordinator_now(),
                        Some(&part_id),
                    )?;
                    continue;
                }
            }
            let mut handle = match recovery.live_handle(&request, &execution) {
                Ok(Some(handle)) => handle,
                Ok(None) => {
                    errors.push(format!(
                        "part {part_id} has no recovery cancellation handle"
                    ));
                    continue;
                }
                Err(error) => {
                    errors.push(format!("part {part_id}: {error}"));
                    continue;
                }
            };
            append(
                self.persistence.as_ref(),
                &plan_id,
                "worker_interrupt_requested",
                self.coordinator_now(),
                Some(&part_id),
            )?;
            let result = handle.cancel();
            match result {
                Ok(()) => {
                    self.coordinator.release_recovered(&request);
                    append(
                        self.persistence.as_ref(),
                        &plan_id,
                        "worker_interrupted",
                        self.coordinator_now(),
                        Some(&part_id),
                    )?;
                }
                Err(error) => errors.push(format!("part {part_id}: {error}")),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Message(format!(
                "scheduler interruption incomplete: {}",
                errors.join("; ")
            )))
        }
    }

    pub fn detach(&mut self) -> Result<()> {
        let plan_id = self.plan_id()?;
        if self.persistence.load(plan_id)?.status.is_terminal() {
            return Ok(());
        }
        append(
            self.persistence.as_ref(),
            plan_id,
            "plan_detached",
            self.coordinator_now(),
            None,
        )
    }

    fn plan_id(&self) -> Result<&str> {
        self.coordinator
            .plan()
            .plan_id
            .as_deref()
            .ok_or_else(|| Error::InvalidState {
                path: "scheduler".into(),
                message: "runtime plan has no id".into(),
            })
    }

    fn release_owned_supervisor(&self, plan_id: &str) -> Result<()> {
        let record = self.persistence.load(plan_id)?;
        if record
            .supervisor_lease
            .as_ref()
            .is_some_and(|lease| lease.owner == self.supervisor_owner)
        {
            self.persistence
                .release_supervisor(plan_id, &self.supervisor_owner)?;
        }
        Ok(())
    }

    fn stop_supervisor(&self, record: &super::super::persistence::PlanRecord) -> Result<()> {
        let Some(lease) = record.supervisor_lease.as_ref() else {
            return Ok(());
        };
        if !lease.is_live(self.coordinator_now()) {
            return Ok(());
        }
        let Some(pid) = lease.pid else {
            return Ok(());
        };
        if pid == std::process::id() {
            return Ok(());
        }
        process_group::terminate_supervisor(pid)
    }
}

fn status_name(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Pending => "pending",
        PlanStatus::Running => "running",
        PlanStatus::Done => "done",
        PlanStatus::Failed => "failed",
        PlanStatus::Interrupted => "interrupted",
        PlanStatus::Killed => "killed",
        PlanStatus::Unknown => "unknown",
    }
}
