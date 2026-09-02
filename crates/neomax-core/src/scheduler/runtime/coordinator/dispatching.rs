use super::super::admission::{AdmissionController, AdmissionDecision};
use super::super::clock::Clock;
use super::super::dispatch::{DispatchError, DispatchPlanner, WorkerRunner};
use super::super::readiness::DependencyReadiness;
use super::super::transitions::{apply_transition, PartTransition};
use super::model::RuntimeCoordinator;
use super::types::TickReport;
use crate::{Error, Result};

const MIN_DEFERRED_RETRY_SECONDS: i64 = 1;
const MAX_DEFERRED_RETRY_SECONDS: i64 = 60;

pub(super) fn dispatch_ready<R, A, C, P>(
    coordinator: &mut RuntimeCoordinator<R, A, C, P>,
    report: &mut TickReport,
) -> Result<()>
where
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock,
    P: DispatchPlanner,
{
    let ready = DependencyReadiness::new(&coordinator.graph, &coordinator.state).ready_ids();
    for part_id in ready {
        if coordinator.active.len() >= coordinator.config.max_live {
            break;
        }
        if coordinator
            .retry_after
            .get(&part_id)
            .is_some_and(|retry_at| *retry_at > coordinator.clock.now())
        {
            continue;
        }
        let part = coordinator
            .plan
            .part(&part_id)
            .ok_or_else(|| Error::NotFound(format!("scheduler part {part_id}")))?;
        let attempt = coordinator
            .attempts
            .get(&part_id)
            .copied()
            .unwrap_or_default()
            + 1;
        let request = match coordinator.planner.plan(&coordinator.plan, part, attempt) {
            Ok(request) => request,
            Err(error) => {
                apply_terminal_failure(coordinator, &part_id, error.to_string(), report)?;
                continue;
            }
        };
        match coordinator
            .admission
            .admit(&request, coordinator.active.len())
        {
            AdmissionDecision::CapacityExhausted { active, maximum } => {
                defer(
                    coordinator,
                    &part_id,
                    format!("scheduler capacity exhausted ({active}/{maximum})"),
                    None,
                    report,
                );
                break;
            }
            AdmissionDecision::AreaBusy { areas } => {
                defer(
                    coordinator,
                    &part_id,
                    format!("scheduler areas unavailable: {}", areas.join(", ")),
                    None,
                    report,
                );
                continue;
            }
            AdmissionDecision::Admitted { .. } => {}
        }
        match coordinator.runner.dispatch_classified(request.clone()) {
            Ok(receipt) => {
                if receipt.run_id.is_empty() {
                    let _ = coordinator.runner.cancel(&request.run_id);
                    coordinator.admission.release_after_cancel(&request);
                    apply_terminal_failure(
                        coordinator,
                        &part_id,
                        "worker dispatcher returned an empty run id".into(),
                        report,
                    )?;
                    continue;
                }
                if let Err(error) = coordinator.state.mark_running(
                    &part_id,
                    receipt.run_id.clone(),
                    receipt.branch.clone(),
                    receipt.profile.clone(),
                    receipt.launched_at as f64,
                ) {
                    let _ = coordinator.runner.cancel(&receipt.run_id);
                    coordinator.admission.release_after_cancel(&request);
                    return Err(error);
                }
                coordinator.attempts.insert(part_id.clone(), attempt);
                coordinator.retry_after.remove(&part_id);
                coordinator.active.insert(
                    part_id.clone(),
                    super::model::ActivePart {
                        request,
                        worker_run_id: receipt.run_id,
                        attempt,
                        worker: super::model::ActiveWorker::Managed,
                    },
                );
                report.launched.push(part_id);
            }
            Err(DispatchError::Deferred { reason, retry_at }) => {
                coordinator.admission.release(&request);
                defer(coordinator, &part_id, reason, retry_at, report);
            }
            Err(DispatchError::Terminal { reason }) => {
                coordinator.admission.release(&request);
                apply_terminal_failure(coordinator, &part_id, reason, report)?;
            }
        }
    }
    Ok(())
}

fn defer<R, A, C, P>(
    coordinator: &mut RuntimeCoordinator<R, A, C, P>,
    part_id: &str,
    _reason: String,
    retry_at: Option<i64>,
    report: &mut TickReport,
) where
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock,
    P: DispatchPlanner,
{
    let now = coordinator.clock.now();
    let minimum_retry_at = now.saturating_add(MIN_DEFERRED_RETRY_SECONDS);
    let maximum_retry_at = now.saturating_add(MAX_DEFERRED_RETRY_SECONDS);
    let retry_at = retry_at
        .unwrap_or(minimum_retry_at)
        .max(minimum_retry_at)
        .min(maximum_retry_at);
    coordinator.retry_after.insert(part_id.to_owned(), retry_at);
    report.retried.push(part_id.to_owned());
}

fn apply_terminal_failure<R, A, C, P>(
    coordinator: &mut RuntimeCoordinator<R, A, C, P>,
    part_id: &str,
    error: String,
    report: &mut TickReport,
) -> Result<()>
where
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock,
    P: DispatchPlanner,
{
    coordinator.retry_after.remove(part_id);
    apply_transition(
        &mut coordinator.state,
        part_id,
        PartTransition::Fail { error },
    )?;
    report.failed.push(part_id.to_owned());
    Ok(())
}
