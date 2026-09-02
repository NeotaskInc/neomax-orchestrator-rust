use anyhow::{Result, bail};
use neomax_core::git::{
    ArtifactCleanupMode, ArtifactCleanupReport, GitWorktreeManager, ManagedArtifactCleaner,
    WorktreeCleanupPolicy, WorktreeOutcome, WorktreeTarget,
};
use neomax_core::runs::RunRecord;
use neomax_core::shepherd::{GitInspectionRequest, GitInspector};

use crate::context::RuntimeContext;

pub(super) fn worktree_target(run: &RunRecord) -> Result<Option<WorktreeTarget>> {
    let Some(worktree) = run.worktree.clone() else {
        return Ok(None);
    };
    let Some(repository) = run.repo.clone() else {
        bail!("run {} has a worktree but no repository", run.id);
    };
    let Some(branch) = run.branch.clone() else {
        bail!("run {} has a worktree but no branch", run.id);
    };
    let base = run
        .base_ref
        .clone()
        .or_else(|| run.base.clone())
        .ok_or_else(|| anyhow::anyhow!("run {} has a worktree but no base ref", run.id))?;
    Ok(Some(WorktreeTarget::new(
        repository, worktree, branch, base,
    )))
}

pub(super) fn force_cleanup(
    context: &RuntimeContext,
    target: &WorktreeTarget,
) -> Result<WorktreeOutcome> {
    if !target.worktree.is_dir() {
        return Ok(WorktreeOutcome::Vanished);
    }
    let repository = target.repository.canonicalize()?;
    let worktree = target.worktree.canonicalize()?;
    if repository == worktree || !worktree.starts_with(&context.paths.worktrees) {
        bail!("refusing force cleanup outside Neomax-managed worktrees");
    }
    let output = neomax_core::git::invoke(
        &repository,
        [
            "worktree",
            "remove",
            "--force",
            "--",
            worktree.to_string_lossy().as_ref(),
        ],
    )?;
    if !output.success {
        bail!("could not remove worktree: {}", output.stderr_text());
    }
    let branch =
        neomax_core::git::invoke(&repository, ["branch", "-D", "--", target.branch.as_str()])?;
    if !branch.success {
        bail!(
            "could not remove branch {}: {}",
            target.branch,
            branch.stderr_text()
        );
    }
    let _ = neomax_core::git::invoke(&repository, ["worktree", "prune"])?;
    Ok(WorktreeOutcome::Cleaned)
}

pub(super) fn cleanup_artifacts(
    context: &RuntimeContext,
    target: &WorktreeTarget,
    mode: ArtifactCleanupMode,
) -> Result<ArtifactCleanupReport> {
    Ok(ManagedArtifactCleaner::new(context.paths.worktrees.clone()).cleanup(target, mode)?)
}

pub(super) fn artifact_report_json(report: &ArtifactCleanupReport) -> serde_json::Value {
    serde_json::json!({
        "found": report.found,
        "eligible": report.eligible,
        "removed": report.removed,
        "skipped": report.skipped,
        "bytes_reclaimable": report.bytes_reclaimable,
        "bytes_reclaimed": report.bytes_reclaimed,
    })
}

pub(super) fn merged_and_clean(run: &RunRecord) -> Result<bool> {
    let Some(repository) = run.repo.as_ref() else {
        return Ok(false);
    };
    let Some(branch) = run.branch.as_deref() else {
        return Ok(false);
    };
    let Some(base) = run.base_ref.as_deref().or(run.base.as_deref()) else {
        return Ok(false);
    };
    let request = GitInspectionRequest::new(repository.clone())
        .branch(branch.to_owned())
        .base(base.to_owned());
    let inspection = GitInspector::new().inspect(&request)?;
    if !(inspection.branch_is_ancestor_of_base && inspection.ahead == 0) {
        return Ok(false);
    }
    let Some(target) = worktree_target(run)? else {
        return Ok(true);
    };
    match GitWorktreeManager.inspect_and_cleanup(&target, WorktreeCleanupPolicy::keep())? {
        WorktreeOutcome::Vanished | WorktreeOutcome::EmptyKept => Ok(true),
        WorktreeOutcome::Cleaned | WorktreeOutcome::HasChanges { .. } => Ok(false),
    }
}
