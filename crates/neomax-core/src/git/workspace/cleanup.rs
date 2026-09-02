use std::ffi::OsStr;
use std::path::Path;

use crate::git::invoke;
use crate::{Error, Result};

use super::identity::{repository_identity, verify_worktree};
use super::types::{IntegrationWorkspace, PartWorkspace, WorkspaceLayout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrationCleanup {
    Removed,
    RetainedDirty,
    AlreadyAbsent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartCleanup {
    Removed,
    RetainedDirty,
    RetainedChanges,
    AlreadyAbsent,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GitWorkspaceCleanup;

impl GitWorkspaceCleanup {
    pub fn remove_clean_integration(
        &self,
        workspace: &IntegrationWorkspace,
    ) -> Result<IntegrationCleanup> {
        let identity = repository_identity(&workspace.repository)?;
        verify_repository_path(&identity.root, &workspace.repository)?;
        let layout = WorkspaceLayout::new(&workspace.worktrees_root);
        let expected = layout.integration_path(&workspace.plan_id)?;
        ensure_expected_path(&expected, &workspace.path)?;
        if !workspace.path.exists() {
            return Ok(IntegrationCleanup::AlreadyAbsent);
        }
        verify_worktree(&workspace.path, &identity, &workspace.branch)?;
        if !is_clean(&workspace.path)? {
            return Ok(IntegrationCleanup::RetainedDirty);
        }
        remove_worktree(&workspace.repository, &workspace.path)?;
        Ok(IntegrationCleanup::Removed)
    }

    pub fn remove_clean_part(&self, workspace: &PartWorkspace) -> Result<PartCleanup> {
        let identity = repository_identity(&workspace.repository)?;
        verify_repository_path(&identity.root, &workspace.repository)?;
        let layout = WorkspaceLayout::new(&workspace.worktrees_root);
        let expected = layout.part_path(&workspace.plan_id, &workspace.part_id)?;
        ensure_expected_path(&expected, &workspace.path)?;
        if !workspace.path.exists() {
            return Ok(PartCleanup::AlreadyAbsent);
        }
        verify_worktree(&workspace.path, &identity, &workspace.branch)?;
        if !is_clean(&workspace.path)? {
            return Ok(PartCleanup::RetainedDirty);
        }
        if commits_ahead(
            &workspace.repository,
            &workspace.integration_branch,
            &workspace.branch,
        )? > 0
        {
            return Ok(PartCleanup::RetainedChanges);
        }
        remove_worktree(&workspace.repository, &workspace.path)?;
        let deleted = invoke(
            &workspace.repository,
            [
                OsStr::new("branch"),
                OsStr::new("-d"),
                OsStr::new("--"),
                OsStr::new(&workspace.branch),
            ],
        )?;
        if !deleted.success {
            return Err(Error::Message(deleted.stderr_text()));
        }
        Ok(PartCleanup::Removed)
    }
}

fn is_clean(worktree: &Path) -> Result<bool> {
    let status = invoke(
        worktree,
        [OsStr::new("status"), OsStr::new("--porcelain=v1")],
    )?;
    if !status.success {
        return Err(Error::Message(status.stderr_text()));
    }
    Ok(status.stdout.is_empty())
}

fn remove_worktree(repository: &Path, worktree: &Path) -> Result<()> {
    let removed = invoke(
        repository,
        [
            OsStr::new("worktree"),
            OsStr::new("remove"),
            OsStr::new("--"),
            worktree.as_os_str(),
        ],
    )?;
    if !removed.success {
        return Err(Error::Message(removed.stderr_text()));
    }
    Ok(())
}

fn ensure_expected_path(expected: &Path, actual: &Path) -> Result<()> {
    if let Ok(metadata) = std::fs::symlink_metadata(actual) {
        if metadata.file_type().is_symlink() {
            return Err(Error::Conflict(
                "refusing cleanup of symlink worktree".into(),
            ));
        }
    }
    let expected = normalized_path(expected)?;
    let actual = normalized_path(actual)?;
    if actual != expected {
        return Err(Error::Conflict(format!(
            "refusing cleanup of unexpected worktree path: {}",
            actual.display()
        )));
    }
    Ok(())
}

fn normalized_path(path: &Path) -> Result<std::path::PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidArgument("worktree path has no parent".into()))?
        .canonicalize()?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::InvalidArgument("worktree path has no name".into()))?;
    Ok(parent.join(name))
}

fn verify_repository_path(actual: &Path, recorded: &Path) -> Result<()> {
    let recorded = recorded.canonicalize()?;
    if actual != recorded {
        return Err(Error::Conflict(format!(
            "repository identity mismatch: expected {}, got {}",
            recorded.display(),
            actual.display()
        )));
    }
    Ok(())
}

fn commits_ahead(repository: &Path, base: &str, branch: &str) -> Result<u64> {
    let output = invoke(
        repository,
        [
            OsStr::new("rev-list"),
            OsStr::new("--count"),
            OsStr::new(&format!("{base}..{branch}")),
        ],
    )?;
    if !output.success {
        return Err(Error::Message(output.stderr_text()));
    }
    output
        .stdout_text()
        .parse::<u64>()
        .map_err(|error| Error::Message(format!("invalid Git ahead count: {error}")))
}
