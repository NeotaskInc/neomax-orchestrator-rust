use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeTarget {
    pub repository: PathBuf,
    pub worktree: PathBuf,
    pub branch: String,
    pub base: String,
}

impl WorktreeTarget {
    pub fn new(
        repository: impl Into<PathBuf>,
        worktree: impl Into<PathBuf>,
        branch: impl Into<String>,
        base: impl Into<String>,
    ) -> Self {
        Self {
            repository: repository.into(),
            worktree: worktree.into(),
            branch: branch.into(),
            base: base.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorktreeCleanupPolicy {
    pub remove_unchanged: bool,
}

impl WorktreeCleanupPolicy {
    pub const fn keep() -> Self {
        Self {
            remove_unchanged: false,
        }
    }

    pub const fn remove_unchanged() -> Self {
        Self {
            remove_unchanged: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInspection {
    pub dirty: bool,
    pub commits_ahead: u64,
    pub files_touched: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeOutcome {
    Vanished,
    Cleaned,
    EmptyKept,
    HasChanges { inspection: WorktreeInspection },
}
