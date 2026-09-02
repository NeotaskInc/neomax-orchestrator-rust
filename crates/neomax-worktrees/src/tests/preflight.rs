use tempfile::tempdir;

use super::fixtures::{context, repository, run};
use crate::git::ProcessGit;
use crate::operations;

#[test]
fn rejects_a_branch_checked_out_elsewhere_before_creating_any_set() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    run(&ProcessGit, &second, &["branch", "samp/feature"]);
    let other = temp.path().join("existing");
    run(
        &ProcessGit,
        &second,
        &["worktree", "add", other.to_str().unwrap(), "samp/feature"],
    );
    let context = context(temp.path(), &[("service-a", first), ("service-b", second)]);
    let error = operations::create(&context, "feature", None, Some("main"), false, &ProcessGit)
        .unwrap_err();
    assert!(error.to_string().contains("already checked out"));
    assert!(!context.worktree_root.join("feature").exists());
}

#[test]
fn rejects_a_missing_base_without_creating_any_set() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let context = context(temp.path(), &[("service-a", first)]);
    let error = operations::create(
        &context,
        "feature",
        None,
        Some("does-not-exist"),
        false,
        &ProcessGit,
    )
    .unwrap_err();
    assert!(error.to_string().contains("base ref"));
    assert!(!context.worktree_root.join("feature").exists());
}
