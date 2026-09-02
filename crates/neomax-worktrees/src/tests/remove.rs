use std::fs;

use tempfile::tempdir;

use super::fixtures::{context, repository};
use crate::git::ProcessGit;
use crate::operations;

#[test]
fn removes_a_clean_set_and_leaves_no_task_directory() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    let context = context(temp.path(), &[("service-a", first), ("service-b", second)]);
    operations::create(&context, "feature", None, Some("main"), false, &ProcessGit).unwrap();
    let report = operations::remove(&context, "feature", false, &ProcessGit).unwrap();
    assert_eq!(report.removed.len(), 2);
    assert!(!context.worktree_root.join("feature").exists());
}

#[test]
fn refuses_dirty_removal_before_removing_any_repository() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    let context = context(temp.path(), &[("service-a", first), ("service-b", second)]);
    operations::create(&context, "feature", None, Some("main"), false, &ProcessGit).unwrap();
    fs::write(
        context.worktree_root.join("feature/service-b/dirty.txt"),
        "keep me\n",
    )
    .unwrap();
    let error = operations::remove(&context, "feature", false, &ProcessGit).unwrap_err();
    assert!(error.to_string().contains("uncommitted changes"));
    assert!(context.worktree_root.join("feature/service-a").is_dir());
    assert!(context.worktree_root.join("feature/service-b").is_dir());
}

#[test]
fn dry_run_remove_keeps_the_set() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let context = context(temp.path(), &[("service-a", first.clone())]);
    operations::create(&context, "feature", None, Some("main"), false, &ProcessGit).unwrap();
    let report = operations::remove(&context, "feature", true, &ProcessGit).unwrap();
    assert_eq!(report.removed.len(), 1);
    assert!(context.worktree_root.join("feature/service-a").is_dir());
}

#[test]
fn refuses_a_clean_worktree_with_committed_work_ahead_of_base() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let context = context(temp.path(), &[("service-a", first.clone())]);
    operations::create(&context, "feature", None, Some("main"), false, &ProcessGit).unwrap();
    let worktree = context.worktree_root.join("feature/service-a");
    fs::write(worktree.join("committed.txt"), "preserve me\n").unwrap();
    super::fixtures::run(&ProcessGit, &worktree, &["add", "committed.txt"]);
    super::fixtures::run(&ProcessGit, &worktree, &["commit", "-qm", "work"]);
    let error = operations::remove(&context, "feature", false, &ProcessGit).unwrap_err();
    assert!(error.to_string().contains("not integrated into base"));
    assert!(worktree.is_dir());
    assert!(
        crate::git::checked_text(
            &ProcessGit,
            &first,
            &crate::git::args(["rev-parse", "--verify", "samp/feature"]),
        )
        .is_ok()
    );
}

#[test]
fn removal_can_use_an_explicit_base_ref() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    super::fixtures::run(&ProcessGit, &first, &["branch", "release"]);
    let context = context(temp.path(), &[("service-a", first)]);
    operations::create(
        &context,
        "feature",
        None,
        Some("release"),
        false,
        &ProcessGit,
    )
    .unwrap();
    let report =
        operations::remove_with_base(&context, "feature", Some("release"), false, &ProcessGit)
            .unwrap();
    assert_eq!(report.removed.len(), 1);
    assert!(!context.worktree_root.join("feature").exists());
}

#[test]
fn committed_work_in_any_repository_blocks_the_entire_removal() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    let context = context(temp.path(), &[("service-a", first), ("service-b", second)]);
    operations::create(&context, "feature", None, Some("main"), false, &ProcessGit).unwrap();
    let worktree = context.worktree_root.join("feature/service-b");
    fs::write(worktree.join("committed.txt"), "preserve me\n").unwrap();
    super::fixtures::run(&ProcessGit, &worktree, &["add", "committed.txt"]);
    super::fixtures::run(&ProcessGit, &worktree, &["commit", "-qm", "work"]);
    let error = operations::remove(&context, "feature", false, &ProcessGit).unwrap_err();
    assert!(error.to_string().contains("not integrated into base"));
    assert!(context.worktree_root.join("feature/service-a").is_dir());
    assert!(context.worktree_root.join("feature/service-b").is_dir());
}
