use neomax_core::{Error, Result};

use crate::discovery::ProjectContext;
use crate::git::{GitRunner, args};
use crate::plan::WorktreeSpec;

use super::preflight;
use super::transaction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoveReport {
    pub task: String,
    pub removed: Vec<WorktreeSpec>,
}

pub fn remove<G: GitRunner>(
    context: &ProjectContext,
    task: &str,
    dry_run: bool,
    git: &G,
) -> Result<RemoveReport> {
    remove_with_base(context, task, None, dry_run, git)
}

pub fn remove_with_base<G: GitRunner>(
    context: &ProjectContext,
    task: &str,
    base: Option<&str>,
    dry_run: bool,
    git: &G,
) -> Result<RemoveReport> {
    let plan = preflight::remove(context, task, base, git)?;
    if !dry_run {
        let mut removed = Vec::new();
        for spec in &plan.specs {
            let result = git.run(
                &spec.repository,
                &args([
                    "worktree",
                    "remove",
                    "--",
                    spec.path.to_string_lossy().as_ref(),
                ]),
            );
            match result {
                Ok(output) if output.success => removed.push(spec.clone()),
                Ok(output) => {
                    let notes = transaction::rollback_after_remove_failure(&removed, spec, git);
                    return Err(transaction::removal_failure(
                        Error::Message(format!(
                            "remove worktree {} failed: {}",
                            spec.label,
                            output.stderr_text()
                        )),
                        notes,
                    ));
                }
                Err(error) => {
                    let notes = transaction::rollback_after_remove_failure(&removed, spec, git);
                    return Err(transaction::removal_failure(error, notes));
                }
            }
        }
        if let Err(error) = transaction::remove_empty_set(&plan.set_path) {
            let notes = transaction::rollback_removed(&removed, git);
            return Err(transaction::removal_failure(error, notes));
        }
    }
    Ok(RemoveReport {
        task: plan.task,
        removed: plan.specs,
    })
}
