use std::path::PathBuf;

use neomax_core::{Error, Result};

use crate::discovery::ProjectContext;
use crate::git::{
    GitRunner, args, commits_ahead, default_base, ref_exists, require_success, worktree_registered,
};
use crate::paths::{ensure_descendant, reject_symlink_if_present, validate_task};
use crate::plan::{CreateAction, WorktreeSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemovePlan {
    pub task: String,
    pub set_path: PathBuf,
    pub specs: Vec<WorktreeSpec>,
}

pub(crate) fn remove<G: GitRunner>(
    context: &ProjectContext,
    task: &str,
    base: Option<&str>,
    git: &G,
) -> Result<RemovePlan> {
    validate_task(task)?;
    let set_path = context.worktree_root.join(task);
    reject_symlink_if_present(&context.worktree_root, "worktree root")?;
    reject_symlink_if_present(&set_path, "task worktree set")?;
    ensure_descendant(&context.worktree_root, &set_path, "task worktree set")?;
    if !set_path.is_dir() {
        return Err(Error::NotFound(format!("no worktree set {task}")));
    }

    let mut specs = Vec::new();
    for repository in &context.repositories {
        let path = set_path.join(&repository.label);
        reject_symlink_if_present(&path, "repository worktree")?;
        if !path.exists() {
            continue;
        }
        if !path.is_dir() || path.symlink_metadata()?.file_type().is_symlink() {
            return Err(Error::Conflict(format!(
                "refusing removal of non-directory worktree path {}",
                path.display()
            )));
        }
        if !worktree_registered(git, &repository.root, &path)? {
            return Err(Error::Conflict(format!(
                "refusing removal of path not registered as a worktree: {}",
                path.display()
            )));
        }
        let status = require_success(
            git.run(&path, &args(["status", "--porcelain=v1"]))?,
            &format!("inspect worktree {}", repository.relative.display()),
        )?;
        if !status.stdout.is_empty() {
            return Err(Error::Conflict(format!(
                "{} has uncommitted changes; refusing removal",
                repository.relative.display()
            )));
        }
        let branch = git
            .run(&path, &args(["branch", "--show-current"]))
            .ok()
            .filter(|output| output.success)
            .map(|output| output.stdout_text())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                Error::Conflict(format!(
                    "refusing removal of detached or unreadable worktree {}",
                    path.display()
                ))
            })?;
        let base = base
            .map(str::to_owned)
            .map(Ok)
            .unwrap_or_else(|| default_base(git, &repository.root))?;
        if !ref_exists(git, &repository.root, &base)? {
            return Err(Error::NotFound(format!(
                "base ref {base} is not available in {}",
                repository.relative.display()
            )));
        }
        let ahead = commits_ahead(git, &repository.root, &base, &branch)?;
        if ahead > 0 {
            return Err(Error::Conflict(format!(
                "{} has {ahead} committed change(s) not integrated into base {base}; refusing removal",
                repository.relative.display()
            )));
        }
        specs.push(WorktreeSpec {
            repository: repository.root.clone(),
            relative_repository: repository.relative.clone(),
            label: repository.label.clone(),
            path,
            branch,
            base,
            action: CreateAction::ReuseWorktree,
        });
    }
    Ok(RemovePlan {
        task: task.to_owned(),
        set_path,
        specs,
    })
}
