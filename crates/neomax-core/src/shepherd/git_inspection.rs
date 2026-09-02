use std::path::{Path, PathBuf};

use crate::{git::inspection as shared, Result};

use super::{git_runner::ProcessGitRunner, MergeReadinessInput};

pub use shared::GitInspectionRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInspection {
    pub repository_root: PathBuf,
    pub branch: String,
    pub base: String,
    pub base_ref: String,
    pub head_sha: String,
    pub base_sha: String,
    pub branch_is_ancestor_of_base: bool,
    pub ahead: u64,
}

impl From<shared::GitInspection> for GitInspection {
    fn from(value: shared::GitInspection) -> Self {
        Self {
            repository_root: value.repository_root,
            branch: value.branch,
            base: value.base,
            base_ref: value.base_ref,
            head_sha: value.head_sha,
            base_sha: value.base_sha,
            branch_is_ancestor_of_base: value.branch_is_ancestor_of_base,
            ahead: value.ahead,
        }
    }
}

impl GitInspection {
    pub fn readiness_input(&self, expected_head_sha: Option<String>) -> MergeReadinessInput {
        let mut input = MergeReadinessInput::local(
            self.branch.clone(),
            self.base.clone(),
            self.head_sha.clone(),
            self.ahead,
        );
        input.branch_is_ancestor_of_base = self.branch_is_ancestor_of_base;
        input.expected_sha = expected_head_sha;
        input
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GitInspector<R = ProcessGitRunner> {
    inner: shared::GitInspector<R>,
}

impl GitInspector<ProcessGitRunner> {
    pub fn new() -> Self {
        Self {
            inner: shared::GitInspector::new(),
        }
    }
}

impl<R> GitInspector<R> {
    pub fn with_runner(runner: R) -> Self {
        Self {
            inner: shared::GitInspector::with_runner(runner),
        }
    }
}

impl<R: super::git_runner::GitCommandRunner> GitInspector<R> {
    pub fn inspect(&self, request: &GitInspectionRequest) -> Result<GitInspection> {
        self.inner.inspect(request).map(Into::into)
    }

    pub fn repository_root(&self, cwd: &Path) -> Result<PathBuf> {
        self.inner.repository_root(cwd)
    }

    pub fn current_branch(&self, repository: &Path) -> Result<String> {
        self.inner.current_branch(repository)
    }

    pub fn resolve_commit(&self, repository: &Path, reference: &str, kind: &str) -> Result<String> {
        self.inner.resolve_commit(repository, reference, kind)
    }

    pub fn fetch_origin(
        &self,
        repository: &Path,
        base: &str,
    ) -> Result<super::git_runner::GitCommandOutput> {
        self.inner.fetch_origin(repository, base)
    }

    pub fn commits_behind_origin(&self, repository: &Path, base: &str) -> Result<u64> {
        self.inner.commits_behind_origin(repository, base)
    }
}
