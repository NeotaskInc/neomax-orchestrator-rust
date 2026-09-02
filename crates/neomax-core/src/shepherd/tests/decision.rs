use serde_json::json;

use crate::shepherd::{
    AlreadyMergedSource, BlockedReason, MergePolicy, MergeState, PullRequestState,
    ReadyDestination, ShepherdDecision, ShepherdStatus, StoppedReason, WaitingReason,
    evaluate_merge_readiness,
};

use super::fixtures::{input, open_pr};

#[test]
fn local_ready_decision_is_exactly_local_and_has_no_pr_metadata() {
    let decision = evaluate_merge_readiness(&input(3), MergePolicy::default());
    assert_eq!(
        decision,
        ShepherdDecision::Ready {
            branch: "feature".into(),
            base: "main".into(),
            ahead: 3,
            destination: ReadyDestination::Local,
            ignored_non_running: Vec::new(),
            url: None,
        }
    );
    assert_eq!(decision.status(), ShepherdStatus::Ready);
}

#[test]
fn ready_pull_request_ignores_non_running_checks_by_default() {
    let decision = evaluate_merge_readiness(
        &input(2).pull_request(open_pr(
            MergeState::Clean,
            Some(vec![
                json!({"name":"actions", "conclusion":"startup_failure"}),
            ]),
        )),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Ready {
            destination: ReadyDestination::PullRequest,
            ignored_non_running,
            ..
        } if ignored_non_running == vec!["actions"]
    ));
}

#[test]
fn disabling_billing_ignore_makes_non_running_checks_blocking() {
    let decision = evaluate_merge_readiness(
        &input(2).pull_request(open_pr(
            MergeState::Clean,
            Some(vec![json!({"name":"actions", "conclusion":"cancelled"})]),
        )),
        MergePolicy {
            ignore_non_running_ci: false,
        },
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Blocked {
            reason: BlockedReason::NonRunningChecks { names },
            ..
        } if names == vec!["actions"]
    ));
}

#[test]
fn pending_checks_wait() {
    let decision = evaluate_merge_readiness(
        &input(1).pull_request(open_pr(
            MergeState::Clean,
            Some(vec![json!({"name":"integration", "status":"in_progress"})]),
        )),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Waiting {
            reason: WaitingReason::RunningChecks { names },
            ..
        } if names == vec!["integration"]
    ));
}

#[test]
fn absent_checks_wait_when_merge_state_has_not_reported_them() {
    let decision = evaluate_merge_readiness(
        &input(1).pull_request(open_pr(MergeState::Unknown, None)),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Waiting {
            reason: WaitingReason::ChecksNotReported {
                merge_state: MergeState::Unknown
            },
            ..
        }
    ));
}

#[test]
fn unrecognized_check_shape_waits_instead_of_allowing_merge() {
    let decision = evaluate_merge_readiness(
        &input(1).pull_request(open_pr(
            MergeState::Clean,
            Some(vec![json!({"name":"new-check-type"})]),
        )),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Waiting {
            reason: WaitingReason::RunningChecks { names },
            ..
        } if names == vec!["new-check-type"]
    ));
}

#[test]
fn genuine_failures_block_even_when_the_name_mentions_billing() {
    let decision = evaluate_merge_readiness(
        &input(1).pull_request(open_pr(
            MergeState::Clean,
            Some(vec![
                json!({"name":"billing-check", "conclusion":"failure"}),
            ]),
        )),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Blocked {
            reason: BlockedReason::FailingChecks { names },
            ..
        } if names == vec!["billing-check"]
    ));
}

#[test]
fn dirty_and_behind_pull_requests_require_rebase() {
    for merge_state in [MergeState::Dirty, MergeState::Behind] {
        let decision = evaluate_merge_readiness(
            &input(1).pull_request(open_pr(merge_state.clone(), None)),
            MergePolicy::default(),
        );
        assert!(matches!(
            decision,
            ShepherdDecision::Blocked {
                reason: BlockedReason::RebaseRequired { merge_state: actual },
                ..
            } if actual == merge_state
        ));
    }
}

#[test]
fn closed_pull_request_is_blocked() {
    let mut pull_request = open_pr(MergeState::Clean, None);
    pull_request.state = PullRequestState::Closed;
    let decision =
        evaluate_merge_readiness(&input(1).pull_request(pull_request), MergePolicy::default());
    assert!(matches!(
        decision,
        ShepherdDecision::Blocked {
            reason: BlockedReason::PullRequestClosed,
            ..
        }
    ));
}

#[test]
fn unknown_pull_request_state_is_blocked_fail_closed() {
    let mut pull_request = open_pr(MergeState::Clean, None);
    pull_request.state = PullRequestState::Unknown;
    let decision =
        evaluate_merge_readiness(&input(1).pull_request(pull_request), MergePolicy::default());
    assert!(matches!(
        decision,
        ShepherdDecision::Blocked {
            reason: BlockedReason::PullRequestStateUnknown,
            ..
        }
    ));
}

#[test]
fn unknown_merge_state_waits_when_only_non_running_checks_are_ignored() {
    let decision = evaluate_merge_readiness(
        &input(1).pull_request(open_pr(
            MergeState::Unknown,
            Some(vec![json!({
                "name": "actions",
                "conclusion": "startup_failure"
            })]),
        )),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Waiting {
            reason: WaitingReason::ChecksNotReported {
                merge_state: MergeState::Unknown
            },
            ..
        }
    ));
}

#[test]
fn moved_head_stops_before_pull_request_work() {
    let decision = evaluate_merge_readiness(
        &input(1)
            .expected_sha("old-sha")
            .pull_request(open_pr(MergeState::Clean, None)),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Stopped {
            reason: StoppedReason::HeadMoved { expected, actual },
            ..
        } if expected == "old-sha" && actual == "abc123"
    ));
}

#[test]
fn merged_states_are_idempotent_and_no_ahead_is_distinct() {
    let git_decision =
        evaluate_merge_readiness(&input(0).ancestor_of_base(), MergePolicy::default());
    assert!(matches!(
        git_decision,
        ShepherdDecision::AlreadyMerged {
            source: AlreadyMergedSource::GitBaseContainsBranch,
            ..
        }
    ));

    let mut pull_request = open_pr(MergeState::Clean, None);
    pull_request.state = PullRequestState::Merged;
    let pr_decision =
        evaluate_merge_readiness(&input(3).pull_request(pull_request), MergePolicy::default());
    assert!(matches!(
        pr_decision,
        ShepherdDecision::AlreadyMerged {
            source: AlreadyMergedSource::PullRequest,
            ..
        }
    ));

    let no_ahead = evaluate_merge_readiness(&input(0), MergePolicy::default());
    assert!(matches!(no_ahead, ShepherdDecision::NothingAhead { .. }));
}

#[test]
fn blocked_merge_state_is_checked_after_ci() {
    let decision = evaluate_merge_readiness(
        &input(1).pull_request(open_pr(MergeState::Blocked, None)),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Blocked {
            reason: BlockedReason::MergeStateBlocked,
            ..
        }
    ));
}
