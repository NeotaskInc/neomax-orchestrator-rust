use super::super::persistence::{PlanStatus, PlanTransition};
use super::super::runtime::{AdmissionController, Clock, WorkerRunner};
use super::super::{PartState, PlanState};
use super::events::{append, event_with_fields};
use super::model::RunAllService;
use super::ports::{PersistencePort, WorkspacePort};
use crate::{Error, Result};

impl<P, W, R, A, C> RunAllService<P, W, R, A, C>
where
    P: PersistencePort,
    W: WorkspacePort,
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock + Clone,
{
    pub(super) fn ensure_supervisor(&self) -> Result<()> {
        let plan_id =
            self.coordinator
                .plan()
                .plan_id
                .as_deref()
                .ok_or_else(|| Error::InvalidState {
                    path: "scheduler".into(),
                    message: "runtime plan has no id".into(),
                })?;
        self.persistence.acquire_supervisor(
            plan_id,
            &self.supervisor_owner,
            Some(std::process::id()),
            self.coordinator_now(),
            super::start::supervisor_lease_seconds(),
        )?;
        Ok(())
    }

    pub(super) fn persist_state(&self) -> Result<()> {
        let plan_id =
            self.coordinator
                .plan()
                .plan_id
                .as_deref()
                .ok_or_else(|| Error::InvalidState {
                    path: "scheduler".into(),
                    message: "runtime plan has no id".into(),
                })?;
        let current = self.persistence.load(plan_id)?;
        if current.status.is_terminal() && !self.coordinator.state().finished() {
            return Err(Error::InvalidState {
                path: plan_id.into(),
                message: "durable plan is terminal while runtime state is live".into(),
            });
        }

        let mut desired = current.clone();
        append_state_change_events(
            self.persistence.as_ref(),
            plan_id,
            &current.state,
            self.coordinator.state(),
            self.coordinator_now(),
        )?;
        desired.state = self.coordinator.state().clone();
        desired.plan = self.coordinator.plan().clone();
        desired.repository = desired.plan.repo.clone();
        desired.base = desired.plan.base.clone();
        desired.integration_branch = desired.plan.integration_branch.clone();
        desired.updated_at = self.coordinator_now();
        self.persistence.save_owned(
            &desired,
            &self.supervisor_owner,
            self.coordinator_now(),
            super::start::supervisor_lease_seconds(),
        )?;
        append(
            self.persistence.as_ref(),
            plan_id,
            "part_state_persisted",
            self.coordinator_now(),
            None,
        )?;

        if desired.state.finished() && !current.status.is_terminal() {
            let (transition, event_status) = if desired
                .state
                .states
                .values()
                .all(|state| *state == PartState::Done)
            {
                (
                    PlanTransition::Done {
                        at: self.coordinator_now(),
                    },
                    PlanStatus::Done,
                )
            } else {
                (
                    PlanTransition::Failed {
                        error: "one or more scheduler parts reached a failed terminal state".into(),
                        at: self.coordinator_now(),
                    },
                    PlanStatus::Failed,
                )
            };
            append(
                self.persistence.as_ref(),
                plan_id,
                "plan_terminal_transition_requested",
                self.coordinator_now(),
                None,
            )?;
            self.persistence.transition(plan_id, transition)?;
            let mut event = event_with_fields(
                plan_id,
                "plan_terminal",
                self.coordinator_now(),
                None,
                [("status".into(), serde_json::json!(event_status))],
            )?;
            event.status = Some(event_status);
            self.persistence.append_event(&event)?;
            self.persistence
                .release_supervisor(plan_id, &self.supervisor_owner)?;
        }
        Ok(())
    }
}

fn append_state_change_events<P: PersistencePort>(
    persistence: &P,
    plan_id: &str,
    previous: &PlanState,
    next: &PlanState,
    now: i64,
) -> Result<()> {
    for (part_id, state) in &next.states {
        if previous.states.get(part_id) != Some(state) {
            persistence.append_event(&event_with_fields(
                plan_id,
                "part_state_change_requested",
                now,
                Some(part_id),
                [("state".into(), serde_json::json!(json_state(*state)))],
            )?)?;
        }
    }
    Ok(())
}

fn json_state(state: PartState) -> &'static str {
    match state {
        PartState::Pending => "pending",
        PartState::Running => "running",
        PartState::Done => "done",
        PartState::Failed => "failed",
        PartState::Conflict => "conflict",
        PartState::Blocked => "blocked",
        PartState::Unknown => "unknown",
    }
}
