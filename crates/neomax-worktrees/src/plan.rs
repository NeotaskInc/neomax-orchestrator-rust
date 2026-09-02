use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use neomax_core::git::workspace::validate_ref_name;
use neomax_core::{Error, Result};

use crate::discovery::{ProjectContext, RepositorySpec, is_git_repository};
use crate::git::{
    GitRunner, args, branch_checked_out, default_base, ref_exists, worktree_registered,
};
use crate::paths::{ensure_descendant, reject_symlink_if_present, validate_task};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateAction {
    CreateBranch,
    UseBranch,
    ReuseWorktree,
    SkipNotGit,
}

impl CreateAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CreateBranch => "create-branch",
            Self::UseBranch => "use-branch",
            Self::ReuseWorktree => "reuse-worktree",
            Self::SkipNotGit => "skip-not-git",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeSpec {
    pub repository: PathBuf,
    pub relative_repository: PathBuf,
    pub label: String,
    pub path: PathBuf,
    pub branch: String,
    pub base: String,
    pub action: CreateAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePlan {
    pub task: String,
    pub set_path: PathBuf,
    pub branch: String,
    pub specs: Vec<WorktreeSpec>,
}

pub fn create<G: GitRunner>(
    context: &ProjectContext,
    task: &str,
    branch: Option<&str>,
    base: Option<&str>,
    git: &G,
) -> Result<CreatePlan> {
    validate_task(task)?;
    let branch = branch
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}/{}", context.branch_prefix, task));
    validate_ref_name(&branch)?;
    let set_path = context.worktree_root.join(task);
    reject_symlink_if_present(&context.worktree_root, "worktree root")?;
    reject_symlink_if_present(&set_path, "task worktree set")?;
    if context.worktree_root.exists() && !context.worktree_root.is_dir() {
        return Err(Error::Conflict(format!(
            "worktree root is not a directory: {}",
            context.worktree_root.display()
        )));
    }
    if set_path.exists() && !set_path.is_dir() {
        return Err(Error::Conflict(format!(
            "task worktree set is not a directory: {}",
            set_path.display()
        )));
    }
    ensure_descendant(&context.worktree_root, &set_path, "task worktree set")?;
    let mut labels = BTreeMap::new();
    let mut specs = Vec::new();
    for repository in &context.repositories {
        if let Some(previous) = labels.insert(&repository.label, &repository.relative) {
            return Err(Error::Conflict(format!(
                "repository labels collide: {} and {} both map to {}",
                previous.display(),
                repository.relative.display(),
                repository.label
            )));
        }
        specs.push(spec_for(repository, &set_path, &branch, base, git)?);
    }
    if specs
        .iter()
        .all(|spec| spec.action == CreateAction::SkipNotGit)
    {
        return Err(Error::NotFound(format!(
            "no Git repositories found under {}",
            context.root.display()
        )));
    }
    Ok(CreatePlan {
        task: task.to_owned(),
        set_path,
        branch,
        specs,
    })
}

fn spec_for<G: GitRunner>(
    repository: &RepositorySpec,
    set_path: &Path,
    branch: &str,
    base: Option<&str>,
    git: &G,
) -> Result<WorktreeSpec> {
    let path = set_path.join(&repository.label);
    reject_symlink_if_present(&path, "repository worktree")?;
    ensure_descendant(set_path, &path, "repository worktree")?;
    if path.exists() {
        if !path.is_dir() || path.symlink_metadata()?.file_type().is_symlink() {
            return Err(Error::Conflict(format!(
                "worktree path exists but is not a directory: {}",
                path.display()
            )));
        }
        if !worktree_registered(git, &repository.root, &path)? {
            return Err(Error::Conflict(format!(
                "refusing to reuse a directory that is not a Git worktree: {}",
                path.display()
            )));
        }
        let branch_name = current_branch(git, &path).ok_or_else(|| {
            Error::Conflict(format!(
                "refusing to reuse a detached or unreadable worktree: {}",
                path.display()
            ))
        })?;
        if branch_name != branch {
            return Err(Error::Conflict(format!(
                "existing worktree {} is on branch {}, expected {}",
                path.display(),
                branch_name,
                branch
            )));
        }
        let base = resolve_base(repository, base, git)?;
        return Ok(WorktreeSpec {
            repository: repository.root.clone(),
            relative_repository: repository.relative.clone(),
            label: repository.label.clone(),
            path,
            branch: branch_name,
            base,
            action: CreateAction::ReuseWorktree,
        });
    }
    if !is_git_repository(git, &repository.root)? {
        return Ok(WorktreeSpec {
            repository: repository.root.clone(),
            relative_repository: repository.relative.clone(),
            label: repository.label.clone(),
            path,
            branch: branch.to_owned(),
            base: base.unwrap_or("main").to_owned(),
            action: CreateAction::SkipNotGit,
        });
    }
    let base = resolve_base(repository, base, git)?;
    let action = if ref_exists(git, &repository.root, branch)? {
        if branch_checked_out(git, &repository.root, branch)? {
            return Err(Error::Conflict(format!(
                "branch {branch} is already checked out in {}",
                repository.relative.display()
            )));
        }
        CreateAction::UseBranch
    } else {
        CreateAction::CreateBranch
    };
    Ok(WorktreeSpec {
        repository: repository.root.clone(),
        relative_repository: repository.relative.clone(),
        label: repository.label.clone(),
        path,
        branch: branch.to_owned(),
        base,
        action,
    })
}

fn resolve_base<G: GitRunner>(
    repository: &RepositorySpec,
    base: Option<&str>,
    git: &G,
) -> Result<String> {
    let base = base
        .map(str::to_owned)
        .map(Ok)
        .unwrap_or_else(|| default_base(git, &repository.root))?;
    validate_ref_name(&base)?;
    if !ref_exists(git, &repository.root, &base)? {
        return Err(Error::NotFound(format!(
            "base ref {base} is not available in {}",
            repository.relative.display()
        )));
    }
    Ok(base)
}

fn current_branch<G: GitRunner>(git: &G, path: &Path) -> Option<String> {
    let output = git.run(path, &args(["branch", "--show-current"])).ok()?;
    output
        .success
        .then(|| output.stdout_text())
        .filter(|value| !value.is_empty())
}
