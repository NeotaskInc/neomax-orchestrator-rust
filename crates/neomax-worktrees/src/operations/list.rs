use std::fs;
use std::path::PathBuf;

use neomax_core::Result;

use crate::discovery::ProjectContext;
use crate::git::{GitRunner, args};
use crate::paths::{reject_symlink_if_present, validate_task};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListReport {
    pub entries: Vec<WorktreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub task: String,
    pub repository: String,
    pub path: PathBuf,
    pub branch: String,
}

pub fn list<G: GitRunner>(context: &ProjectContext, git: &G) -> Result<ListReport> {
    reject_symlink_if_present(&context.worktree_root, "worktree root")?;
    if !context.worktree_root.is_dir() {
        return Ok(ListReport {
            entries: Vec::new(),
        });
    }
    let mut entries = Vec::new();
    for task in fs::read_dir(&context.worktree_root)? {
        let task = task?;
        let task_path = task.path();
        if !task_path.is_dir() || task_path.symlink_metadata()?.file_type().is_symlink() {
            continue;
        }
        let task_name = task.file_name().to_string_lossy().into_owned();
        if validate_task(&task_name).is_err() {
            continue;
        }
        for repository in &context.repositories {
            let path = task_path.join(&repository.label);
            reject_symlink_if_present(&path, "repository worktree")?;
            if !path.is_dir() || path.symlink_metadata()?.file_type().is_symlink() {
                continue;
            }
            let branch = git
                .run(&path, &args(["branch", "--show-current"]))
                .ok()
                .filter(|output| output.success)
                .map(|output| output.stdout_text())
                .unwrap_or_default();
            entries.push(WorktreeEntry {
                task: task_name.clone(),
                repository: repository.relative.to_string_lossy().into_owned(),
                path,
                branch,
            });
        }
    }
    entries.sort_by(|left, right| {
        (&left.task, &left.repository, &left.path).cmp(&(
            &right.task,
            &right.repository,
            &right.path,
        ))
    });
    Ok(ListReport { entries })
}
