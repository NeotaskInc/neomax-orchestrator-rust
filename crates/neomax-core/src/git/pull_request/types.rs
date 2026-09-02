use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestRequest {
    pub repository: PathBuf,
    pub branch: String,
    pub base: Option<String>,
    pub title: Option<String>,
    pub result_text: Option<String>,
    pub run_id: Option<String>,
    pub profile: Option<String>,
    pub expected_head_sha: Option<String>,
    pub draft: bool,
}

impl PullRequestRequest {
    pub fn branch(repository: impl Into<PathBuf>, branch: impl Into<String>) -> Self {
        Self {
            repository: repository.into(),
            branch: branch.into(),
            base: None,
            title: None,
            result_text: None,
            run_id: None,
            profile: None,
            expected_head_sha: None,
            draft: true,
        }
    }

    pub fn base(mut self, base: impl Into<String>) -> Self {
        self.base = Some(base.into());
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn result_text(mut self, result_text: impl Into<String>) -> Self {
        self.result_text = Some(result_text.into());
        self
    }

    pub fn run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    pub fn profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    pub fn expected_head_sha(mut self, expected: impl Into<String>) -> Self {
        self.expected_head_sha = Some(expected.into());
        self
    }

    pub fn draft(mut self, draft: bool) -> Self {
        self.draft = draft;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestReceipt {
    pub url: String,
    pub number: Option<u64>,
    pub state: Option<String>,
    pub branch: String,
    pub base: String,
    pub reused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRequestOutcome {
    Opened(PullRequestReceipt),
    Existing(PullRequestReceipt),
    AlreadyMerged {
        branch: String,
        base: String,
    },
    Stopped {
        branch: String,
        expected: String,
        actual: String,
    },
}

impl PullRequestOutcome {
    pub fn receipt(&self) -> Option<&PullRequestReceipt> {
        match self {
            Self::Opened(receipt) | Self::Existing(receipt) => Some(receipt),
            Self::AlreadyMerged { .. } | Self::Stopped { .. } => None,
        }
    }

    pub fn url(&self) -> Option<&str> {
        self.receipt().map(|receipt| receipt.url.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingPullRequest {
    pub url: String,
    pub number: Option<u64>,
    pub state: Option<String>,
}
