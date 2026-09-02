use std::path::{Path, PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationStatus {
    Created,
    Reused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceLayout {
    pub worktrees_root: PathBuf,
}

impl WorkspaceLayout {
    pub fn new(worktrees_root: impl Into<PathBuf>) -> Self {
        Self {
            worktrees_root: worktrees_root.into(),
        }
    }

    pub fn integration_path(&self, plan_id: &str) -> Result<PathBuf> {
        validate_component(plan_id, "plan id")?;
        Ok(self.worktrees_root.join(format!("integ-{plan_id}")))
    }

    pub fn part_path(&self, plan_id: &str, part_id: &str) -> Result<PathBuf> {
        validate_component(plan_id, "plan id")?;
        validate_component(part_id, "part id")?;
        Ok(self.worktrees_root.join(format!("{plan_id}-{part_id}")))
    }

    pub fn owns(&self, path: &Path) -> Result<bool> {
        let root = canonical_or_existing(&self.worktrees_root)?;
        let path = canonical_or_existing(path)?;
        Ok(path != root && path.starts_with(&root))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRequest {
    pub repository: PathBuf,
    pub base: Option<String>,
    pub plan_id: String,
    pub integration_branch: Option<String>,
}

impl IntegrationRequest {
    pub fn new(
        repository: impl Into<PathBuf>,
        plan_id: impl Into<String>,
        base: Option<String>,
        integration_branch: Option<String>,
    ) -> Self {
        Self {
            repository: repository.into(),
            base,
            plan_id: plan_id.into(),
            integration_branch,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRequest {
    pub repository: PathBuf,
    pub integration_branch: String,
    pub plan_id: String,
    pub part_id: String,
}

impl PartRequest {
    pub fn new(
        repository: impl Into<PathBuf>,
        integration_branch: impl Into<String>,
        plan_id: impl Into<String>,
        part_id: impl Into<String>,
    ) -> Self {
        Self {
            repository: repository.into(),
            integration_branch: integration_branch.into(),
            plan_id: plan_id.into(),
            part_id: part_id.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationWorkspace {
    pub repository: PathBuf,
    pub base: String,
    pub branch: String,
    pub path: PathBuf,
    pub plan_id: String,
    pub worktrees_root: PathBuf,
    pub status: AllocationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartWorkspace {
    pub repository: PathBuf,
    pub integration_branch: String,
    pub branch: String,
    pub path: PathBuf,
    pub plan_id: String,
    pub part_id: String,
    pub worktrees_root: PathBuf,
    pub status: AllocationStatus,
}

fn validate_component(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || value.starts_with('.')
        || value.ends_with('.')
        || value.contains("..")
    {
        return Err(Error::InvalidArgument(format!(
            "{label} must use [A-Za-z0-9._-] without path traversal"
        )));
    }
    Ok(())
}

fn canonical_or_existing(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        return Err(Error::NotFound(path.display().to_string()));
    }
    Ok(path.canonicalize()?)
}
