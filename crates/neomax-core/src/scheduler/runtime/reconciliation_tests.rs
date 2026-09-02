use super::dispatch::WorkerOutcome;
use super::reconciliation::reconcile;
use super::transitions::PartTransition;

#[test]
fn completion_becomes_done_without_a_retry_window() {
    let result = reconcile(&WorkerOutcome::Completed {
        run_id: "run-1".into(),
    });
    assert_eq!(result.retry_at, None);
    assert_eq!(result.transition, PartTransition::Complete);
}

#[test]
fn provider_limits_become_retries_and_preserve_reset_time() {
    let result = reconcile(&WorkerOutcome::RateLimited {
        run_id: "run-1".into(),
        retry_at: Some(99),
        error: Some("429".into()),
    });
    assert_eq!(result.retry_at, Some(99));
    assert_eq!(
        result.transition,
        PartTransition::Retry {
            reason: "429".into()
        }
    );
}

#[test]
fn conflicts_are_terminal_until_human_resolution() {
    let result = reconcile(&WorkerOutcome::Conflict {
        run_id: "run-1".into(),
        error: "merge conflict".into(),
    });
    assert!(matches!(result.transition, PartTransition::Conflict { .. }));
    assert_eq!(result.retry_at, None);
}
