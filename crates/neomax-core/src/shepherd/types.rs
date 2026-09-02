use serde_json::Value;

use super::CiClassification;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShepherdStatus {
    Ready,
    Waiting,
    Blocked,
    Stopped,
    AlreadyMerged,
    NothingAhead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlreadyMergedSource {
    GitBaseContainsBranch,
    PullRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoppedReason {
    HeadMoved { expected: String, actual: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockedReason {
    RebaseRequired { merge_state: MergeState },
    FailingChecks { names: Vec<String> },
    NonRunningChecks { names: Vec<String> },
    MergeStateBlocked,
    PullRequestClosed,
    PullRequestStateUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitingReason {
    RunningChecks { names: Vec<String> },
    ChecksNotReported { merge_state: MergeState },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyDestination {
    Local,
    PullRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PullRequestState {
    Open,
    Merged,
    Closed,
    #[default]
    Unknown,
}

impl PullRequestState {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_uppercase().as_str() {
            "OPEN" => Self::Open,
            "MERGED" => Self::Merged,
            "CLOSED" => Self::Closed,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MergeState {
    Clean,
    Dirty,
    Behind,
    Blocked,
    #[default]
    Unknown,
    Pending,
    HasHooks,
    Unstable,
}

impl MergeState {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_uppercase().as_str() {
            "CLEAN" => Self::Clean,
            "DIRTY" => Self::Dirty,
            "BEHIND" => Self::Behind,
            "BLOCKED" => Self::Blocked,
            "PENDING" => Self::Pending,
            "HAS_HOOKS" => Self::HasHooks,
            "UNSTABLE" => Self::Unstable,
            _ => Self::Unknown,
        }
    }

    pub fn waits_for_unreported_checks(&self) -> bool {
        matches!(
            self,
            Self::Unknown | Self::Pending | Self::HasHooks | Self::Unstable
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PullRequestSnapshot {
    pub url: Option<String>,
    pub state: PullRequestState,
    pub merge_state: MergeState,
    pub status_check_rollup: Option<Vec<Value>>,
}

impl PullRequestSnapshot {
    pub fn open(merge_state: MergeState, status_check_rollup: Option<Vec<Value>>) -> Self {
        Self {
            url: None,
            state: PullRequestState::Open,
            merge_state,
            status_check_rollup,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn check_classification(&self) -> CiClassification {
        super::classify_ci_checks(self.status_check_rollup.as_deref().unwrap_or_default())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MergeReadinessInput {
    pub branch: String,
    pub base: String,
    pub head_sha: String,
    pub expected_sha: Option<String>,
    pub ahead: u64,
    pub branch_is_ancestor_of_base: bool,
    pub pull_request: Option<PullRequestSnapshot>,
}

impl MergeReadinessInput {
    pub fn local(
        branch: impl Into<String>,
        base: impl Into<String>,
        head_sha: impl Into<String>,
        ahead: u64,
    ) -> Self {
        Self {
            branch: branch.into(),
            base: base.into(),
            head_sha: head_sha.into(),
            expected_sha: None,
            ahead,
            branch_is_ancestor_of_base: false,
            pull_request: None,
        }
    }

    pub fn expected_sha(mut self, expected_sha: impl Into<String>) -> Self {
        self.expected_sha = Some(expected_sha.into());
        self
    }

    pub fn ancestor_of_base(mut self) -> Self {
        self.branch_is_ancestor_of_base = true;
        self
    }

    pub fn pull_request(mut self, pull_request: PullRequestSnapshot) -> Self {
        self.pull_request = Some(pull_request);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShepherdDecision {
    Ready {
        branch: String,
        base: String,
        ahead: u64,
        destination: ReadyDestination,
        ignored_non_running: Vec<String>,
        url: Option<String>,
    },
    Waiting {
        reason: WaitingReason,
        url: Option<String>,
    },
    Blocked {
        reason: BlockedReason,
        url: Option<String>,
    },
    Stopped {
        branch: String,
        reason: StoppedReason,
    },
    AlreadyMerged {
        branch: String,
        base: String,
        source: AlreadyMergedSource,
        url: Option<String>,
    },
    NothingAhead {
        branch: String,
        base: String,
    },
}

impl ShepherdDecision {
    pub fn status(&self) -> ShepherdStatus {
        match self {
            Self::Ready { .. } => ShepherdStatus::Ready,
            Self::Waiting { .. } => ShepherdStatus::Waiting,
            Self::Blocked { .. } => ShepherdStatus::Blocked,
            Self::Stopped { .. } => ShepherdStatus::Stopped,
            Self::AlreadyMerged { .. } => ShepherdStatus::AlreadyMerged,
            Self::NothingAhead { .. } => ShepherdStatus::NothingAhead,
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            Self::Ready { url, .. }
            | Self::Waiting { url, .. }
            | Self::Blocked { url, .. }
            | Self::AlreadyMerged { url, .. } => url.as_deref(),
            Self::Stopped { .. } | Self::NothingAhead { .. } => None,
        }
    }
}
