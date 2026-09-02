use super::super::super::PartState;
use super::super::admission::AdmissionController;
use super::super::clock::Clock;
use super::super::dispatch::{DispatchPlanner, WorkerRunner};
use super::super::reconciliation::reconcile;
use super::super::transitions::{PartTransition, apply_transition};
use super::model::RuntimeCoordinator;
use super::types::TickReport;
use crate::{Error, Result};

pub(super) fn reconcile_completed<R, A, C, P>(
    coordinator: &mut RuntimeCoordinator<R, A, C, P>,
    report: &mut TickReport,
) -> Result<()>
where
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock,
    P: DispatchPlanner,
{
    let active_ids = coordinator.active.keys().cloned().collect::<Vec<_>>();
    for part_id in active_ids {
        let Some(active) = coordinator.active.get(&part_id) else {
            continue;
        };
        let worker_run_id = active.worker_run_id.clone();
        let attempt = active.attempt;
        let request = active.request.clone();
        let Some(outcome) = coordinator.poll_active(&part_id)? else {
            continue;
        };
        if outcome.run_id() != worker_run_id {
            return Err(Error::Conflict(format!(
                "worker outcome run id {} does not match active run {}",
                outcome.run_id(),
                worker_run_id
            )));
        }
        let mut result = reconcile(&outcome);
        if matches!(&result.transition, PartTransition::Retry { .. })
            && attempt >= coordinator.config.max_attempts
        {
            let reason = match result.transition {
                PartTransition::Retry { reason } => reason,
                _ => unreachable!(),
            };
            result.transition = PartTransition::Fail {
                error: format!("retry limit reached: {reason}"),
            };
            result.retry_at = None;
        }
        let applied = apply_transition(&mut coordinator.state, &part_id, result.transition)?;
        coordinator.admission.release(&request);
        coordinator.active.remove(&part_id);
        if applied.current == PartState::Done {
            report.completed.push(part_id);
        } else if applied.current == PartState::Failed {
            report.failed.push(part_id);
        } else if applied.current == PartState::Conflict {
            report.conflicted.push(part_id);
        } else if applied.current == PartState::Pending {
            if let Some(retry_at) = result.retry_at {
                coordinator.retry_after.insert(part_id.clone(), retry_at);
            }
            report.retried.push(part_id);
        }
    }
    Ok(())
}
