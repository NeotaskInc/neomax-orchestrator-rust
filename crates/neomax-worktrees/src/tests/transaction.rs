use std::path::{Path, PathBuf};

use tempfile::tempdir;

use super::fixtures::{context, repository, run};
use crate::git::{GitOutput, GitRunner, ProcessGit};
use crate::operations;

#[test]
fn failed_later_repository_rolls_back_only_new_worktrees_and_branches() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    let context = context(
        temp.path(),
        &[("service-a", first.clone()), ("service-b", second.clone())],
    );
    let git = FailWorktreeAdd {
        repository: second.clone(),
    };
    let error =
        operations::create(&context, "feature", None, Some("main"), false, &git).unwrap_err();
    assert!(error.to_string().contains("create worktree"));
    assert!(!context.worktree_root.join("feature/service-a").exists());
    assert!(!context.worktree_root.join("feature/service-b").exists());
    assert!(!crate::git::ref_exists(&ProcessGit, &first, "samp/feature").unwrap());
    assert!(!crate::git::ref_exists(&ProcessGit, &second, "samp/feature").unwrap());
    assert!(!context.worktree_root.join("feature").exists());
}

#[test]
fn rollback_keeps_preexisting_branches_when_the_set_creation_fails() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    run(&ProcessGit, &first, &["branch", "samp/feature"]);
    run(&ProcessGit, &second, &["branch", "samp/feature"]);
    let context = context(
        temp.path(),
        &[("service-a", first.clone()), ("service-b", second.clone())],
    );
    let error = operations::create(
        &context,
        "feature",
        None,
        Some("main"),
        false,
        &FailWorktreeAdd {
            repository: second.clone(),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("create worktree"));
    assert!(!context.worktree_root.join("feature/service-a").exists());
    assert!(crate::git::ref_exists(&ProcessGit, &first, "samp/feature").unwrap());
    assert!(crate::git::ref_exists(&ProcessGit, &second, "samp/feature").unwrap());
}

#[test]
fn failed_later_removal_restores_worktrees_removed_earlier() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    let context = context(
        temp.path(),
        &[("service-a", first), ("service-b", second.clone())],
    );
    operations::create(&context, "feature", None, Some("main"), false, &ProcessGit).unwrap();
    let error = operations::remove(
        &context,
        "feature",
        false,
        &FailWorktreeRemove { repository: second },
    )
    .unwrap_err();
    assert!(error.to_string().contains("remove worktree"));
    assert!(context.worktree_root.join("feature/service-a").is_dir());
    assert!(context.worktree_root.join("feature/service-b").is_dir());
    assert!(context.worktree_root.join("feature").is_dir());
}

struct FailWorktreeAdd {
    repository: PathBuf,
}

impl GitRunner for FailWorktreeAdd {
    fn run(&self, cwd: &Path, values: &[String]) -> neomax_core::Result<GitOutput> {
        let is_add = values.iter().any(|value| value == "worktree")
            && values.iter().any(|value| value == "add")
            && cwd == self.repository;
        if is_add {
            return Ok(GitOutput {
                success: false,
                stdout: String::new(),
                stderr: "injected add failure".into(),
            });
        }
        ProcessGit.run(cwd, values)
    }
}

struct FailWorktreeRemove {
    repository: PathBuf,
}

impl GitRunner for FailWorktreeRemove {
    fn run(&self, cwd: &Path, values: &[String]) -> neomax_core::Result<GitOutput> {
        let is_remove = values.iter().any(|value| value == "worktree")
            && values.iter().any(|value| value == "remove")
            && cwd == self.repository;
        if is_remove {
            return Ok(GitOutput {
                success: false,
                stdout: String::new(),
                stderr: "injected remove failure".into(),
            });
        }
        ProcessGit.run(cwd, values)
    }
}
