use std::ffi::OsStr;
use std::path::{Component, Path};

use crate::git::inspection::{GitCommandRunner, ProcessGitRunner};
use crate::{Error, Result};

use super::inspection::inspect_with_runner;
use super::state::{WorktreeCleanupPolicy, WorktreeOutcome, WorktreeTarget};

#[derive(Debug, Clone, Copy, Default)]
pub struct GitWorktreeManager;

impl GitWorktreeManager {
    pub fn inspect_and_cleanup(
        &self,
        target: &WorktreeTarget,
        policy: WorktreeCleanupPolicy,
    ) -> Result<WorktreeOutcome> {
        self.inspect_and_cleanup_with_runner(target, policy, &ProcessGitRunner)
    }

    pub fn inspect_and_cleanup_with_runner<R: GitCommandRunner>(
        &self,
        target: &WorktreeTarget,
        policy: WorktreeCleanupPolicy,
        runner: &R,
    ) -> Result<WorktreeOutcome> {
        if !target.worktree.is_dir() {
            return Ok(WorktreeOutcome::Vanished);
        }
        let inspection = inspect_with_runner(
            runner,
            &target.repository,
            &target.worktree,
            &target.base,
            &target.branch,
        )?;
        if !inspection.dirty && inspection.commits_ahead == 0 {
            if policy.remove_unchanged {
                validate_cleanup_target(&target.repository, &target.worktree)?;
                checked(
                    runner,
                    &target.repository,
                    &[
                        OsStr::new("worktree"),
                        OsStr::new("remove"),
                        OsStr::new("--force"),
                        OsStr::new("--"),
                        target.worktree.as_os_str(),
                    ],
                )?;
                checked(
                    runner,
                    &target.repository,
                    &[
                        OsStr::new("branch"),
                        OsStr::new("-D"),
                        OsStr::new("--"),
                        OsStr::new(&target.branch),
                    ],
                )?;
                return Ok(WorktreeOutcome::Cleaned);
            }
            return Ok(WorktreeOutcome::EmptyKept);
        }
        Ok(WorktreeOutcome::HasChanges { inspection })
    }
}

fn checked<R: GitCommandRunner>(runner: &R, repository: &Path, args: &[&OsStr]) -> Result<()> {
    let arguments = args
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let output = runner.run(repository, &arguments)?;
    if output.success {
        Ok(())
    } else {
        Err(Error::Message(output.stderr))
    }
}

fn validate_cleanup_target(repository: &Path, worktree: &Path) -> Result<()> {
    let repository = repository.canonicalize()?;
    let worktree = worktree.canonicalize()?;
    let broad = worktree.parent().is_none()
        || worktree
            .components()
            .filter(|part| matches!(part, Component::Normal(_)))
            .count()
            < 2;
    if broad || repository == worktree {
        return Err(Error::Conflict(format!(
            "refusing unsafe worktree cleanup target {}",
            worktree.display()
        )));
    }
    Ok(())
}
