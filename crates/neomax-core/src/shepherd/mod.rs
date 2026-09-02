mod checks;
mod decision;
mod git_inspection;
mod git_runner;
mod policy;
mod types;

#[cfg(test)]
mod tests;

pub use checks::{CiClassification, classify_ci_checks};
pub use decision::evaluate_merge_readiness;
pub use git_inspection::{GitInspection, GitInspectionRequest, GitInspector};
pub use git_runner::{GitCommandOutput, GitCommandRunner, ProcessGitRunner};
pub use policy::{MergePolicy, billing_ignore_enabled};
pub use types::{
    AlreadyMergedSource, BlockedReason, MergeReadinessInput, MergeState, PullRequestSnapshot,
    PullRequestState, ReadyDestination, ShepherdDecision, ShepherdStatus, StoppedReason,
    WaitingReason,
};
