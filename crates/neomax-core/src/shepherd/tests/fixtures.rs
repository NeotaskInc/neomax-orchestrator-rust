use crate::shepherd::{MergeReadinessInput, MergeState, PullRequestSnapshot};

pub(super) fn input(ahead: u64) -> MergeReadinessInput {
    MergeReadinessInput::local("feature", "main", "abc123", ahead)
}

pub(super) fn open_pr(
    state: MergeState,
    checks: Option<Vec<serde_json::Value>>,
) -> PullRequestSnapshot {
    PullRequestSnapshot::open(state, checks).with_url("https://github.com/example/repo/pull/1")
}
