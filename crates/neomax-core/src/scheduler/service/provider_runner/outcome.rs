use super::super::super::runtime::WorkerOutcome;
use crate::runs::{RunRecord, RunStatus};

pub(super) fn outcome_for_status(
    scheduler_run_id: &str,
    run: &RunRecord,
    status: RunStatus,
) -> WorkerOutcome {
    match status {
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
                .unwrap_or_else(|| format!("worker finished with {}", status.as_str())),
        },
    }
}
