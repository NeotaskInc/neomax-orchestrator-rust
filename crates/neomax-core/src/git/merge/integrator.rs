use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::git::{invoke, output};
use crate::{Error, Result};

use super::resolve_safe_conflicts;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationOutcome {
    AlreadyIntegrated,
    Merged,
    SelfHealed { files: Vec<PathBuf> },
    Conflict { files: Vec<PathBuf> },
}

pub trait PartIntegrator: Send + Sync {
    fn integrate(
        &self,
        repository: &Path,
        integration_worktree: &Path,
        integration_branch: &str,
        part_branch: &str,
        part_id: &str,
    ) -> Result<IntegrationOutcome>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GitPartIntegrator;

impl PartIntegrator for GitPartIntegrator {
    fn integrate(
        &self,
        repository: &Path,
        integration_worktree: &Path,
        integration_branch: &str,
        part_branch: &str,
        part_id: &str,
    ) -> Result<IntegrationOutcome> {
        let range = format!("{integration_branch}..{part_branch}");
        let ahead = output(repository, ["rev-list", "--count", &range])?
            .parse::<u64>()
            .map_err(|_| Error::Message("Git returned an invalid ahead count".into()))?;
        if ahead == 0 {
            return Ok(IntegrationOutcome::AlreadyIntegrated);
        }
        let message = format!("integrate {part_id} ({part_branch})");
        let merged = invoke(
            integration_worktree,
            [
                OsStr::new("merge"),
                OsStr::new("--no-ff"),
                OsStr::new("-m"),
                OsStr::new(&message),
                OsStr::new("--"),
                OsStr::new(part_branch),
            ],
        )?;
        if merged.success {
            return Ok(IntegrationOutcome::Merged);
        }
        let resolution = resolve_safe_conflicts(integration_worktree)?;
        if resolution.remaining.is_empty() {
            let committed = invoke(integration_worktree, ["commit", "--no-edit"])?;
            if committed.success {
                return Ok(IntegrationOutcome::SelfHealed {
                    files: resolution.resolved,
                });
            }
            abort_merge(integration_worktree)?;
            return Err(Error::Message(format!(
                "resolved merge conflicts but could not commit: {}",
                committed.stderr_text()
            )));
        }
        abort_merge(integration_worktree)?;
        Ok(IntegrationOutcome::Conflict {
            files: resolution.remaining,
        })
    }
}

fn abort_merge(worktree: &Path) -> Result<()> {
    let aborted = invoke(worktree, ["merge", "--abort"])?;
    if !aborted.success {
        return Err(Error::Message(format!(
            "merge failed and could not be aborted: {}",
            aborted.stderr_text()
        )));
    }
    Ok(())
}
