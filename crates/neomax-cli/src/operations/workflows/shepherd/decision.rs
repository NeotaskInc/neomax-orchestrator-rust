use neomax_core::shepherd::{
    AlreadyMergedSource, BlockedReason, ReadyDestination, ShepherdDecision, StoppedReason,
    WaitingReason,
};
use serde_json::json;

pub(crate) fn decision_json(decision: &ShepherdDecision) -> serde_json::Value {
    match decision {
        ShepherdDecision::Ready {
            branch,
            base,
            ahead,
            destination,
            ignored_non_running,
            url,
        } => json!({
            "status": "ready",
            "branch": branch,
            "base": base,
            "ahead": ahead,
            "destination": match destination { ReadyDestination::Local => "local", ReadyDestination::PullRequest => "pull-request" },
            "ignored_non_running": ignored_non_running,
            "url": url,
        }),
        ShepherdDecision::Waiting { reason, url } => json!({
            "status": "waiting",
            "reason": waiting_reason(reason),
            "url": url,
        }),
        ShepherdDecision::Blocked { reason, url } => json!({
            "status": "blocked",
            "reason": blocked_reason(reason),
            "url": url,
        }),
        ShepherdDecision::Stopped { branch, reason } => json!({
            "status": "stopped",
            "branch": branch,
            "reason": stopped_reason(reason),
        }),
        ShepherdDecision::AlreadyMerged {
            branch,
            base,
            source,
            url,
        } => json!({
            "status": "already-merged",
            "branch": branch,
            "base": base,
            "source": match source { AlreadyMergedSource::GitBaseContainsBranch => "git-base-contains-branch", AlreadyMergedSource::PullRequest => "pull-request" },
            "url": url,
        }),
        ShepherdDecision::NothingAhead { branch, base } => json!({
            "status": "nothing-ahead",
            "branch": branch,
            "base": base,
        }),
    }
}

pub(crate) fn decision_text(decision: &ShepherdDecision) -> String {
    match decision {
        ShepherdDecision::Ready {
            branch,
            base,
            ahead,
            destination,
            ignored_non_running,
            ..
        } => {
            let destination = match destination {
                ReadyDestination::Local => "local",
                ReadyDestination::PullRequest => "pull request",
            };
            let note = if ignored_non_running.is_empty() {
                String::new()
            } else {
                format!(
                    "; ignored non-running CI: {}",
                    ignored_non_running.join(", ")
                )
            };
            format!("ready ({destination}): {branch} is {ahead} commit(s) ahead of {base}{note}")
        }
        ShepherdDecision::Waiting { reason, .. } => format!("waiting: {}", waiting_reason(reason)),
        ShepherdDecision::Blocked { reason, .. } => format!("blocked: {}", blocked_reason(reason)),
        ShepherdDecision::Stopped { branch, reason } => {
            format!("stopped: {branch}: {}", stopped_reason(reason))
        }
        ShepherdDecision::AlreadyMerged { branch, base, .. } => {
            format!("merged: {branch} is already contained in {base}")
        }
        ShepherdDecision::NothingAhead { branch, base } => {
            format!("nothing to merge: {branch} has no commits ahead of {base}")
        }
    }
}

fn waiting_reason(reason: &WaitingReason) -> String {
    match reason {
        WaitingReason::RunningChecks { names } => {
            format!("CI checks still running: {}", names.join(", "))
        }
        WaitingReason::ChecksNotReported { merge_state } => {
            format!("CI not reported yet (merge state: {merge_state:?})")
        }
    }
}

fn blocked_reason(reason: &BlockedReason) -> String {
    match reason {
        BlockedReason::RebaseRequired { merge_state } => {
            format!("PR needs rebase (merge state: {merge_state:?})")
        }
        BlockedReason::FailingChecks { names } => {
            format!("failing CI checks: {}", names.join(", "))
        }
        BlockedReason::NonRunningChecks { names } => {
            format!("non-running checks are blocking: {}", names.join(", "))
        }
        BlockedReason::MergeStateBlocked => "PR merge state is blocked".into(),
        BlockedReason::PullRequestClosed => "pull request is closed".into(),
        BlockedReason::PullRequestStateUnknown => "pull request state is unknown".into(),
    }
}

fn stopped_reason(reason: &StoppedReason) -> String {
    match reason {
        StoppedReason::HeadMoved { expected, actual } => {
            format!("HEAD moved ({actual} != expected {expected})")
        }
    }
}
