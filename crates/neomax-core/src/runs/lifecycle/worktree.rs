use std::path::PathBuf;

use crate::git::{
    ArtifactCleanupMode, GitWorktreeManager, ManagedArtifactCleaner, WorktreeCleanupPolicy,
    WorktreeOutcome, WorktreeTarget,
};
use crate::runs::{RunRecord, RunStatus};
use crate::{Error, Result};

const ARTIFACT_CLEANUP_EXTRA: &str = "worktree_artifact_cleanup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeState {
    Cleaned,
    EmptyKept,
    HasChanges,
    Vanished,
    InspectionFailed,
}

impl WorktreeState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cleaned => "cleaned",
            Self::EmptyKept => "empty_kept",
            Self::HasChanges => "has_changes",
            Self::Vanished => "vanished",
            Self::InspectionFailed => "inspection_failed",
        }
    }
}

pub trait WorktreeFinalizer: Send + Sync {
    fn record_outcome(&self, run: &mut RunRecord) -> Result<()>;
}

pub struct ManagedRunWorktreeFinalizer {
    managed_root: PathBuf,
}

impl ManagedRunWorktreeFinalizer {
    pub fn new(managed_root: impl Into<PathBuf>) -> Self {
        Self {
            managed_root: managed_root.into(),
        }
    }
}

impl<F> WorktreeFinalizer for F
where
    F: Fn(&mut RunRecord) -> Result<()> + Send + Sync,
{
    fn record_outcome(&self, run: &mut RunRecord) -> Result<()> {
        self(run)
    }
}

impl WorktreeFinalizer for GitWorktreeManager {
    fn record_outcome(&self, run: &mut RunRecord) -> Result<()> {
        record_git_outcome(self, run)?;
        Ok(())
    }
}

impl WorktreeFinalizer for ManagedRunWorktreeFinalizer {
    fn record_outcome(&self, run: &mut RunRecord) -> Result<()> {
        record_artifact_cleanup(&self.managed_root, run)?;
        record_git_outcome(&GitWorktreeManager, run)?;
        Ok(())
    }
}

fn record_git_outcome(
    manager: &GitWorktreeManager,
    run: &mut RunRecord,
) -> Result<()> {
    let Some(worktree) = run.worktree.clone() else {
        return Ok(());
    };
    if !worktree.is_dir() {
        run.worktree_state = Some(WorktreeState::Vanished.as_str().into());
        return Ok(());
    }
    run.worktree_state = Some(WorktreeState::InspectionFailed.as_str().into());
    let target = target_for(run, worktree)?;
    let policy = cleanup_policy(run);
    let outcome = manager
        .inspect_and_cleanup(&target, policy)
        .inspect_err(|_error| {
            run.worktree_state = Some(WorktreeState::InspectionFailed.as_str().into());
        })?;
    match outcome {
        WorktreeOutcome::Vanished => {
            run.worktree_state = Some(WorktreeState::Vanished.as_str().into());
        }
        WorktreeOutcome::Cleaned => {
            run.worktree_state = Some(WorktreeState::Cleaned.as_str().into());
            run.files_touched.clear();
        }
        WorktreeOutcome::EmptyKept => {
            run.worktree_state = Some(WorktreeState::EmptyKept.as_str().into());
            run.files_touched.clear();
        }
        WorktreeOutcome::HasChanges { inspection } => {
            run.worktree_state = Some(WorktreeState::HasChanges.as_str().into());
            run.files_touched = inspection.files_touched.into_iter().collect();
        }
    }
    Ok(())
}

fn record_artifact_cleanup(managed_root: &std::path::Path, run: &mut RunRecord) -> Result<()> {
    if !should_cleanup_artifacts(run) {
        return Ok(());
    }
    let Some(worktree) = run.worktree.clone() else {
        return Ok(());
    };
    if !worktree.is_dir() {
        return Ok(());
    }
    let target = target_for(run, worktree)?;
    let report = ManagedArtifactCleaner::new(managed_root)
        .cleanup(&target, ArtifactCleanupMode::Apply)?;
    run.extra.insert(
        ARTIFACT_CLEANUP_EXTRA.into(),
        serde_json::json!({
            "found": report.found,
            "eligible": report.eligible,
            "removed": report.removed,
            "skipped": report.skipped,
            "bytes_reclaimable": report.bytes_reclaimable,
            "bytes_reclaimed": report.bytes_reclaimed,
        }),
    );
    Ok(())
}

fn should_cleanup_artifacts(run: &RunRecord) -> bool {
    matches!(run.status, RunStatus::Done | RunStatus::Error) && !run.killed
}

fn cleanup_policy(run: &RunRecord) -> WorktreeCleanupPolicy {
    if should_cleanup_artifacts(run) && !run.open_pull_request {
        WorktreeCleanupPolicy::remove_unchanged()
    } else {
        WorktreeCleanupPolicy::keep()
    }
}

fn target_for(run: &RunRecord, worktree: std::path::PathBuf) -> Result<WorktreeTarget> {
    let repository = run
        .repo
        .clone()
        .ok_or_else(|| Error::InvalidArgument("worktree run is missing repo".into()))?;
    let branch = run
        .branch
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::InvalidArgument("worktree run is missing branch".into()))?;
    let base = run
        .base
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::InvalidArgument("worktree run is missing base".into()))?;
    Ok(WorktreeTarget::new(repository, worktree, branch, base))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use super::*;
    use crate::Engine;

    fn git(directory: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn run(status: RunStatus) -> RunRecord {
        let mut run = RunRecord::new(
            "run",
            Engine::Codex,
            "model",
            "task",
            "profile",
            "worktree",
            1,
        );
        run.status = status;
        run
    }

    #[test]
    fn automatic_whole_cleanup_requires_a_safe_unreviewed_terminal_run() {
        let done = run(RunStatus::Done);
        assert_eq!(
            cleanup_policy(&done),
            WorktreeCleanupPolicy::remove_unchanged()
        );

        let mut review = done.clone();
        review.open_pull_request = true;
        assert_eq!(cleanup_policy(&review), WorktreeCleanupPolicy::keep());

        let mut killed = done;
        killed.killed = true;
        assert_eq!(cleanup_policy(&killed), WorktreeCleanupPolicy::keep());
        assert_eq!(
            cleanup_policy(&run(RunStatus::Limit)),
            WorktreeCleanupPolicy::keep()
        );
    }

    #[test]
    fn terminal_finalizer_removes_generated_artifacts_before_whole_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        let managed_root = temp.path().join("worktrees");
        let worktree = managed_root.join("run");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&managed_root).unwrap();
        git(&repository, &["init", "-q"]);
        git(&repository, &["config", "user.email", "fixture@example.com"]);
        git(&repository, &["config", "user.name", "Fixture"]);
        fs::write(repository.join(".gitignore"), "node_modules/\n").unwrap();
        git(&repository, &["add", ".gitignore"]);
        git(&repository, &["commit", "-qm", "base"]);
        let base = git(&repository, &["branch", "--show-current"]);
        git(&repository, &["branch", "task"]);
        git(
            &repository,
            &["worktree", "add", "-q", worktree.to_str().unwrap(), "task"],
        );
        fs::create_dir_all(worktree.join("node_modules/pkg")).unwrap();
        fs::write(
            worktree.join("node_modules/pkg/index.js"),
            "generated\n",
        )
        .unwrap();

        let mut run = run(RunStatus::Done);
        run.repo = Some(repository);
        run.worktree = Some(worktree.clone());
        run.branch = Some("task".into());
        run.base = Some(base);
        ManagedRunWorktreeFinalizer::new(managed_root)
            .record_outcome(&mut run)
            .unwrap();

        assert_eq!(run.worktree_state.as_deref(), Some("cleaned"));
        assert_eq!(run.extra[ARTIFACT_CLEANUP_EXTRA]["removed"], 1);
        assert!(!worktree.exists());
    }
}
