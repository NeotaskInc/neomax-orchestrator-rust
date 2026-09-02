use std::fs;

use neomax_core::Result;

use crate::discovery::ProjectContext;
use crate::git::GitRunner;
use crate::plan::{self, CreateAction, CreatePlan, WorktreeSpec};

use super::transaction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateReport {
    pub plan: CreatePlan,
    pub created: Vec<WorktreeSpec>,
    pub reused: Vec<WorktreeSpec>,
    pub skipped: Vec<WorktreeSpec>,
}

pub fn create<G: GitRunner>(
    context: &ProjectContext,
    task: &str,
    branch: Option<&str>,
    base: Option<&str>,
    dry_run: bool,
    git: &G,
) -> Result<CreateReport> {
    let plan = plan::create(context, task, branch, base, git)?;
    let mut created = Vec::new();
    let mut reused = Vec::new();
    let mut skipped = Vec::new();
    let set_path_existed = plan.set_path.exists();
    if !dry_run {
        if let Err(error) = fs::create_dir_all(&plan.set_path) {
            transaction::cleanup_empty_set(&plan.set_path, set_path_existed);
            return Err(error.into());
        }
    }
    for spec in &plan.specs {
        match spec.action {
            CreateAction::CreateBranch | CreateAction::UseBranch => {
                if dry_run {
                    created.push(spec.clone());
                    continue;
                }
                if let Err(error) = transaction::add_worktree(spec, git) {
                    created.push(spec.clone());
                    return Err(transaction::failed_create(
                        error,
                        &created,
                        &plan.set_path,
                        set_path_existed,
                        git,
                    ));
                }
                created.push(spec.clone());
            }
            CreateAction::ReuseWorktree => reused.push(spec.clone()),
            CreateAction::SkipNotGit => skipped.push(spec.clone()),
        }
    }
    Ok(CreateReport {
        plan,
        created,
        reused,
        skipped,
    })
}
