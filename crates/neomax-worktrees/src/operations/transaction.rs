use std::fs;
use std::path::Path;

use neomax_core::{Error, Result};

use crate::git::{
    GitRunner, args, branch_checked_out, ref_commit, ref_exists, require_success,
    worktree_registered,
};
use crate::plan::{CreateAction, WorktreeSpec};

pub(crate) fn add_worktree<G: GitRunner>(spec: &WorktreeSpec, git: &G) -> Result<()> {
    let output = if spec.action == CreateAction::UseBranch {
        git.run(
            &spec.repository,
            &args([
                "worktree",
                "add",
                "--",
                spec.path.to_string_lossy().as_ref(),
                spec.branch.as_str(),
            ]),
        )?
    } else {
        git.run(
            &spec.repository,
            &args([
                "worktree",
                "add",
                "-b",
                spec.branch.as_str(),
                "--",
                spec.path.to_string_lossy().as_ref(),
                spec.base.as_str(),
            ]),
        )?
    };
    require_success(output, &format!("create worktree {}", spec.label)).map(|_| ())
}

pub(crate) fn failed_create<G: GitRunner>(
    error: Error,
    created: &[WorktreeSpec],
    set_path: &Path,
    set_path_existed: bool,
    git: &G,
) -> Error {
    let notes = rollback_created(created, git);
    cleanup_empty_set(set_path, set_path_existed);
    if notes.is_empty() {
        error
    } else {
        Error::Message(format!("{error}; rollback: {}", notes.join("; ")))
    }
}

fn rollback_created<G: GitRunner>(created: &[WorktreeSpec], git: &G) -> Vec<String> {
    let mut notes = Vec::new();
    for spec in created.iter().rev() {
        let registered = match worktree_registered(git, &spec.repository, &spec.path) {
            Ok(value) => value,
            Err(error) => {
                notes.push(format!("{} could not be inspected: {error}", spec.label));
                false
            }
        };
        let path_exists = spec.path.exists();
        if registered {
            match git.run(
                &spec.repository,
                &args([
                    "worktree",
                    "remove",
                    "--",
                    spec.path.to_string_lossy().as_ref(),
                ]),
            ) {
                Ok(output) if output.success => {}
                Ok(output) => notes.push(format!(
                    "{} worktree retained during rollback: {}",
                    spec.label,
                    output.stderr_text()
                )),
                Err(error) => notes.push(format!(
                    "{} worktree retained during rollback: {error}",
                    spec.label
                )),
            }
        } else if path_exists {
            notes.push(format!(
                "{} path retained because it is not a registered worktree",
                spec.label
            ));
        }
        if spec.action == CreateAction::CreateBranch && path_entry_absent(&spec.path) {
            match untouched_branch(git, spec) {
                Ok(true) => match git.run(
                    &spec.repository,
                    &args(["branch", "-D", "--", spec.branch.as_str()]),
                ) {
                    Ok(output) if output.success => {}
                    Ok(output) => notes.push(format!(
                        "{} branch retained during rollback: {}",
                        spec.label,
                        output.stderr_text()
                    )),
                    Err(error) => notes.push(format!(
                        "{} branch retained during rollback: {error}",
                        spec.label
                    )),
                },
                Ok(false) => notes.push(format!(
                    "{} branch retained because it changed during rollback",
                    spec.label
                )),
                Err(error) => notes.push(format!(
                    "{} branch could not be inspected during rollback: {error}",
                    spec.label
                )),
            }
        }
    }
    notes
}

fn untouched_branch<G: GitRunner>(git: &G, spec: &WorktreeSpec) -> Result<bool> {
    if !ref_exists(git, &spec.repository, &spec.branch)? {
        return Ok(false);
    }
    if branch_checked_out(git, &spec.repository, &spec.branch)? {
        return Ok(false);
    }
    Ok(ref_commit(git, &spec.repository, &spec.branch)?
        == ref_commit(git, &spec.repository, &spec.base)?)
}

pub(crate) fn cleanup_empty_set(path: &Path, existed: bool) {
    if !existed
        && path.is_dir()
        && fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
    {
        let _ = fs::remove_dir(path);
    }
}

pub(crate) fn remove_empty_set(path: &Path) -> Result<bool> {
    let mut entries = fs::read_dir(path)?;
    if entries.next().transpose()?.is_some() {
        return Ok(false);
    }
    fs::remove_dir(path)?;
    Ok(true)
}

fn path_entry_absent(path: &Path) -> bool {
    fs::symlink_metadata(path).is_err()
}

pub(crate) fn rollback_removed<G: GitRunner>(removed: &[WorktreeSpec], git: &G) -> Vec<String> {
    let mut notes = Vec::new();
    for spec in removed.iter().rev() {
        match worktree_registered(git, &spec.repository, &spec.path) {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => {
                notes.push(format!(
                    "{} could not be inspected for restoration: {error}",
                    spec.label
                ));
                continue;
            }
        }
        match git.run(
            &spec.repository,
            &args([
                "worktree",
                "add",
                "--",
                spec.path.to_string_lossy().as_ref(),
                spec.branch.as_str(),
            ]),
        ) {
            Ok(output) if output.success => {}
            Ok(output) => notes.push(format!(
                "{} could not be restored: {}",
                spec.label,
                output.stderr_text()
            )),
            Err(error) => notes.push(format!("{} could not be restored: {error}", spec.label)),
        }
    }
    notes
}

pub(crate) fn rollback_after_remove_failure<G: GitRunner>(
    removed: &[WorktreeSpec],
    current: &WorktreeSpec,
    git: &G,
) -> Vec<String> {
    let mut attempted = removed.to_vec();
    match worktree_registered(git, &current.repository, &current.path) {
        Ok(false) => attempted.push(current.clone()),
        Ok(true) => {}
        Err(error) => {
            return vec![format!(
                "{} could not be inspected for restoration: {error}",
                current.label
            )];
        }
    }
    rollback_removed(&attempted, git)
}

pub(crate) fn removal_failure(error: Error, notes: Vec<String>) -> Error {
    if notes.is_empty() {
        error
    } else {
        Error::Message(format!("{error}; rollback: {}", notes.join("; ")))
    }
}
