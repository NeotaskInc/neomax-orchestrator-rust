use super::super::persistence::PlanRecord;
use super::super::runtime::{
    apply_transition, reconcile, AdmissionController, Clock, DispatchPlanner, RuntimeCoordinator,
    WorkerOutcome, WorkerRunner,
};
use super::events::{event, event_with_fields};
use super::ports::{PersistencePort, RecoveryPort, RecoveryStatus};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    pub waiting: Vec<String>,
    pub completed: Vec<String>,
    pub failed: Vec<String>,
    pub retried: Vec<String>,
}

pub(crate) fn rehydrate_running_parts<P, Q, D, R, A, C>(
    persistence: &P,
    recovery: &mut Q,
    planner: &D,
    coordinator: &mut RuntimeCoordinator<R, A, C, D>,
    record: &mut PlanRecord,
    now: C,
) -> Result<RecoveryReport>
where
    P: PersistencePort,
    Q: RecoveryPort,
    D: DispatchPlanner,
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock,
{
    if let Some(worktree) = record.worktree.as_ref() {
        if !worktree.is_dir() {
            return Err(Error::NotFound(format!(
                "integration worktree {}",
                worktree.display()
            )));
        }
    }
    let mut report = RecoveryReport::default();
    let running = record
        .state
        .states
        .iter()
        .filter_map(|(id, state)| {
            (*state == super::super::PartState::Running).then_some(id.clone())
        })
        .collect::<Vec<_>>();
    for part_id in running {
        let part = record
            .plan
            .part(&part_id)
            .ok_or_else(|| Error::NotFound(format!("scheduler part {part_id}")))?;
        let execution =
            record
                .state
                .execution(&part_id)
                .cloned()
                .ok_or_else(|| Error::InvalidState {
                    path: record.plan_id.clone().into(),
                    message: format!("running part {part_id} has no execution record"),
                })?;
        let original_run_id = execution
            .run_id
            .clone()
            .ok_or_else(|| Error::InvalidState {
                path: record.plan_id.clone().into(),
                message: format!("running part {part_id} has no run id"),
            })?;
        let mut request = planner.plan(&record.plan, part, 1)?;
        request.run_id = original_run_id.clone();
        if execution.branch.is_some() {
            request.branch = execution.branch.clone();
        }
        persistence.append_event(&event(
            &record.plan_id,
            "recovery_inspection_requested",
            now.now(),
            Some(&part_id),
            None,
        )?)?;
        let status = recovery.inspect(&request, &execution)?;
        persistence.append_event(&event_with_fields(
            &record.plan_id,
            "recovery_inspection_completed",
            now.now(),
            Some(&part_id),
            [("run_id".into(), original_run_id.clone().into())],
        )?)?;
        match status {
            RecoveryStatus::StillRunning => {
                let handle = recovery.live_handle(&request, &execution)?.ok_or_else(|| {
                    Error::InvalidState {
                        path: record.plan_id.clone().into(),
                        message: format!(
                            "recovery reported live part {part_id} without a polling handle"
                        ),
                    }
                })?;
                coordinator.admit_recovered(&request)?;
                if let Err(error) =
                    coordinator.register_recovered(part_id.clone(), request.clone(), 1, handle)
                {
                    coordinator.release_recovered(&request);
                    return Err(error);
                }
                report.waiting.push(part_id);
            }
            RecoveryStatus::Completed(outcome) | RecoveryStatus::Failed(outcome) => {
                ensure_outcome_run(&outcome, &original_run_id)?;
                let result = reconcile(&outcome);
                let applied =
                    apply_transition(coordinator.state_mut(), &part_id, result.transition)?;
                coordinator.release_recovered(&request);
                persistence.append_event(&event_with_fields(
                    &record.plan_id,
                    "recovery_outcome_reconciled",
                    now.now(),
                    Some(&part_id),
                    [("run_id".into(), original_run_id.clone().into())],
                )?)?;
                match applied.current {
                    super::super::PartState::Done => report.completed.push(part_id),
                    super::super::PartState::Failed
                    | super::super::PartState::Conflict
                    | super::super::PartState::Blocked
                    | super::super::PartState::Unknown => report.failed.push(part_id),
                    super::super::PartState::Pending => report.retried.push(part_id),
                    _ => {}
                }
            }
        }
    }
    record.state = coordinator.state().clone();
    record.updated_at = now.now();
    persistence.save(record)?;
    Ok(report)
}

pub fn recover_running_parts<P, Q, D, C>(
    persistence: &P,
    recovery: &mut Q,
    planner: &D,
    record: &mut PlanRecord,
    now: C,
) -> Result<RecoveryReport>
where
    P: PersistencePort,
    Q: RecoveryPort,
    D: DispatchPlanner,
    C: Clock,
{
    if let Some(worktree) = record.worktree.as_ref() {
        if !worktree.is_dir() {
            return Err(Error::NotFound(format!(
                "integration worktree {}",
                worktree.display()
            )));
        }
    }
    let mut report = RecoveryReport::default();
    let running = record
        .state
        .states
        .iter()
        .filter_map(|(id, state)| {
            (*state == super::super::PartState::Running).then_some(id.clone())
        })
        .collect::<Vec<_>>();
    for part_id in running {
        let part = record
            .plan
            .part(&part_id)
            .ok_or_else(|| Error::NotFound(format!("scheduler part {part_id}")))?;
        let execution =
            record
                .state
                .execution(&part_id)
                .cloned()
                .ok_or_else(|| Error::InvalidState {
                    path: record.plan_id.clone().into(),
                    message: format!("running part {part_id} has no execution record"),
                })?;
        let original_run_id = execution
            .run_id
            .clone()
            .ok_or_else(|| Error::InvalidState {
                path: record.plan_id.clone().into(),
                message: format!("running part {part_id} has no run id"),
            })?;
        let mut request = planner.plan(&record.plan, part, 1)?;
        request.run_id = original_run_id.clone();
        if execution.branch.is_some() {
            request.branch = execution.branch.clone();
        }
        persistence.append_event(&event(
            &record.plan_id,
            "recovery_inspection_requested",
            now.now(),
            Some(&part_id),
            None,
        )?)?;
        let status = recovery.inspect(&request, &execution)?;
        persistence.append_event(&event_with_fields(
            &record.plan_id,
            "recovery_inspection_completed",
            now.now(),
            Some(&part_id),
            [("run_id".into(), original_run_id.clone().into())],
        )?)?;
        match status {
            RecoveryStatus::StillRunning => report.waiting.push(part_id),
            RecoveryStatus::Completed(outcome) | RecoveryStatus::Failed(outcome) => {
                ensure_outcome_run(&outcome, &original_run_id)?;
                let result = reconcile(&outcome);
                let transition = result.transition;
                let applied = apply_transition(&mut record.state, &part_id, transition)?;
                match applied.current {
                    super::super::PartState::Done => report.completed.push(part_id),
                    super::super::PartState::Failed
                    | super::super::PartState::Conflict
                    | super::super::PartState::Blocked
                    | super::super::PartState::Unknown => report.failed.push(part_id),
                    super::super::PartState::Pending => report.retried.push(part_id),
                    _ => {}
                }
            }
        }
    }
    record.updated_at = now.now();
    persistence.save(record)?;
    Ok(report)
}

fn ensure_outcome_run(outcome: &WorkerOutcome, expected: &str) -> Result<()> {
    if outcome.run_id() == expected {
        return Ok(());
    }
    Err(Error::Conflict(format!(
        "recovery outcome run id {} does not match {}",
        outcome.run_id(),
        expected
    )))
}
