mod ci;
mod claim;
mod claims;
mod coordinator;
mod event;
mod fingerprint;
mod mirror;
mod schema;
mod status;
mod store;
mod types;

#[cfg(test)]
mod tests;

pub use ci::{
    ci_sync_action, classify_ci_checks, evaluate_merge_gate, CiCheck, CiClassification,
    CiConclusion, CiSyncAction, MergeGate, MergeInput, MergeState, CI_NONRUN_CONCLUSIONS,
    CI_REAL_FAIL_CONCLUSIONS, CI_WORKFLOW_SENTINEL, NEOMAX_CI_WORKFLOW,
};
pub use claims::{ClaimLiveness, ClaimOwnerState, NoLiveClaims, ProcessLiveness};
pub use coordinator::{
    CrossRepoIssueCoordinator, CrossRepoIssueInput, LocalOnlyMirrorDriver, MirrorDriver,
    MirrorRequest, RepositoryCatalog, RepositoryTarget,
};
pub use fingerprint::{find_open_duplicate, issue_fingerprint, normalize_title};
pub use store::{IssueLoadDiagnostic, IssueStore, IssueStoreConfig};
pub use types::{
    Issue, IssueClaim, IssueEvent, IssueMirror, IssueStatus, MirrorState, PullRequestLink,
};
