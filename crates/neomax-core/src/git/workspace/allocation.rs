use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::git::invoke;
use crate::{Error, Result};

use super::branch::{
    branch_commit, ensure_branch, generated_integration_branch, generated_part_branch,
    resolve_default_branch, validate_ref_name,
};
use super::identity::{repository_identity, verify_worktree};
use super::types::{
    AllocationStatus, IntegrationRequest, IntegrationWorkspace, PartRequest, PartWorkspace,
    WorkspaceLayout,
};

#[derive(Debug, Clone)]
pub struct GitWorkspaceAllocator {
    layout: WorkspaceLayout,
}

impl GitWorkspaceAllocator {
    pub fn new(worktrees_root: impl Into<PathBuf>) -> Self {
        Self {
            layout: WorkspaceLayout::new(worktrees_root),
        }
    }

    pub fn layout(&self) -> &WorkspaceLayout {
        &self.layout
    }

    pub fn integration(&self, request: &IntegrationRequest) -> Result<IntegrationWorkspace> {
        let identity = repository_identity(&request.repository)?;
        let base = request
            .base
            .clone()
            .map(|value| validate_ref_name(&value).map(|()| value))
            .transpose()?
            .unwrap_or(resolve_default_branch(&identity.root)?);
        branch_commit_or_reference(&identity.root, &base)?;
        let branch = request
            .integration_branch
            .clone()
            .map(|value| validate_ref_name(&value).map(|()| value))
            .transpose()?
            .unwrap_or(generated_integration_branch(&request.plan_id)?);
        let path = self.layout.integration_path(&request.plan_id)?;
        ensure_workspace_parent(&self.layout.worktrees_root, &path)?;

        let status = if path.exists() {
            verify_worktree(&path, &identity, &branch)?;
            ensure_owned_path(&self.layout, &path)?;
            AllocationStatus::Reused
        } else {
            let branch_status = ensure_branch(&identity.root, &branch, &base)?;
            add_worktree(&identity.root, &path, &branch)?;
            verify_worktree(&path, &identity, &branch)?;
            branch_status
        };
        Ok(IntegrationWorkspace {
            repository: identity.root,
            base,
            branch,
            path,
            plan_id: request.plan_id.clone(),
            worktrees_root: self.layout.worktrees_root.clone(),
            status,
        })
    }

    pub fn part(&self, request: &PartRequest) -> Result<PartWorkspace> {
        let identity = repository_identity(&request.repository)?;
        validate_ref_name(&request.integration_branch)?;
        branch_commit_or_reference(&identity.root, &request.integration_branch)?;
        let branch = generated_part_branch(&request.plan_id, &request.part_id)?;
        let path = self.layout.part_path(&request.plan_id, &request.part_id)?;
        ensure_workspace_parent(&self.layout.worktrees_root, &path)?;
        let status = if path.exists() {
            verify_worktree(&path, &identity, &branch)?;
            ensure_owned_path(&self.layout, &path)?;
            AllocationStatus::Reused
        } else {
            let branch_status =
                ensure_branch(&identity.root, &branch, &request.integration_branch)?;
            add_worktree(&identity.root, &path, &branch)?;
            verify_worktree(&path, &identity, &branch)?;
            branch_status
        };
        Ok(PartWorkspace {
            repository: identity.root,
            integration_branch: request.integration_branch.clone(),
            branch,
            path,
            plan_id: request.plan_id.clone(),
            part_id: request.part_id.clone(),
            worktrees_root: self.layout.worktrees_root.clone(),
            status,
        })
    }
}

fn branch_commit_or_reference(repository: &Path, reference: &str) -> Result<String> {
    if let Ok(commit) = branch_commit(repository, reference) {
        return Ok(commit);
    }
    let result = invoke(
        repository,
        [
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new(&format!("{reference}^{{commit}}")),
        ],
    )?;
    if !result.success {
        return Err(Error::NotFound(format!("Git ref {reference}")));
    }
    Ok(result.stdout_text())
}

fn ensure_workspace_parent(root: &Path, path: &Path) -> Result<()> {
    fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidArgument("workspace path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    if !parent.starts_with(&root) {
        return Err(Error::Conflict(format!(
            "workspace path escapes {}",
            root.display()
        )));
    }
    Ok(())
}

fn ensure_owned_path(layout: &WorkspaceLayout, path: &Path) -> Result<()> {
    if !layout.owns(path)? {
        return Err(Error::Conflict(format!(
            "workspace path is outside the managed root: {}",
            path.display()
        )));
    }
    Ok(())
}

fn add_worktree(repository: &Path, path: &Path, branch: &str) -> Result<()> {
    let result = invoke(
        repository,
        [
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("--"),
            path.as_os_str(),
            OsStr::new(branch),
        ],
    )?;
    if !result.success {
        return Err(Error::Message(result.stderr_text()));
    }
    Ok(())
}
