use super::{
    BlockedReason, MergePolicy, MergeReadinessInput, PullRequestState, ReadyDestination,
    ShepherdDecision, StoppedReason, WaitingReason,
};

pub fn evaluate_merge_readiness(
    input: &MergeReadinessInput,
    policy: MergePolicy,
) -> ShepherdDecision {
    let pull_request = input.pull_request.as_ref();
    let url = pull_request.and_then(|pr| pr.url.clone());

    if input.branch_is_ancestor_of_base && input.ahead == 0 {
        return ShepherdDecision::AlreadyMerged {
            branch: input.branch.clone(),
            base: input.base.clone(),
            source: super::AlreadyMergedSource::GitBaseContainsBranch,
            url,
        };
    }

    if let Some(expected) = input.expected_sha.as_deref() {
        if input.head_sha != expected {
            return ShepherdDecision::Stopped {
                branch: input.branch.clone(),
                reason: StoppedReason::HeadMoved {
                    expected: expected.to_string(),
                    actual: input.head_sha.clone(),
                },
            };
        }
    }

    if input.ahead == 0 {
        return ShepherdDecision::NothingAhead {
            branch: input.branch.clone(),
            base: input.base.clone(),
        };
    }

    let Some(pull_request) = pull_request else {
        return ShepherdDecision::Ready {
            branch: input.branch.clone(),
            base: input.base.clone(),
            ahead: input.ahead,
            destination: ReadyDestination::Local,
            ignored_non_running: Vec::new(),
            url: None,
        };
    };

    if pull_request.state == PullRequestState::Merged {
        return ShepherdDecision::AlreadyMerged {
            branch: input.branch.clone(),
            base: input.base.clone(),
            source: super::AlreadyMergedSource::PullRequest,
            url,
        };
    }

    if pull_request.state == PullRequestState::Closed {
        return ShepherdDecision::Blocked {
            reason: BlockedReason::PullRequestClosed,
            url,
        };
    }

    if pull_request.state == PullRequestState::Unknown {
        return ShepherdDecision::Blocked {
            reason: BlockedReason::PullRequestStateUnknown,
            url,
        };
    }

    if matches!(
        pull_request.merge_state,
        super::MergeState::Dirty | super::MergeState::Behind
    ) {
        return ShepherdDecision::Blocked {
            reason: BlockedReason::RebaseRequired {
                merge_state: pull_request.merge_state.clone(),
            },
            url,
        };
    }

    let checks = pull_request.check_classification();
    let blocking = checks.blocking_failures(policy.ignore_non_running_ci);
    if !blocking.is_empty() {
        let reason = if checks.real_failures.is_empty() {
            BlockedReason::NonRunningChecks { names: blocking }
        } else {
            BlockedReason::FailingChecks { names: blocking }
        };
        return ShepherdDecision::Blocked { reason, url };
    }

    let pending = checks.pending_names();
    if !pending.is_empty() {
        return ShepherdDecision::Waiting {
            reason: WaitingReason::RunningChecks { names: pending },
            url,
        };
    }

    if checks.real_failures.is_empty()
        && checks.pending.is_empty()
        && pull_request.merge_state.waits_for_unreported_checks()
    {
        return ShepherdDecision::Waiting {
            reason: WaitingReason::ChecksNotReported {
                merge_state: pull_request.merge_state.clone(),
            },
            url,
        };
    }

    if pull_request.merge_state == super::MergeState::Blocked {
        return ShepherdDecision::Blocked {
            reason: BlockedReason::MergeStateBlocked,
            url,
        };
    }

    ShepherdDecision::Ready {
        branch: input.branch.clone(),
        base: input.base.clone(),
        ahead: input.ahead,
        destination: ReadyDestination::PullRequest,
        ignored_non_running: checks.ignored_non_running(),
        url,
    }
}
