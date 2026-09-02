use std::path::PathBuf;

use tempfile::tempdir;

use super::fixtures::{context, repository};
use crate::git::ProcessGit;
use crate::operations;
use crate::paths::{relative_repo, validate_task};

#[test]
fn rejects_task_traversal_and_absolute_repository_paths() {
    assert!(validate_task("../escape").is_err());
    assert!(validate_task("nested/task").is_err());
    assert!(relative_repo(&PathBuf::from("../repo")).is_err());
    assert!(relative_repo(&PathBuf::from("/tmp/repo")).is_err());
    #[cfg(windows)]
    {
        assert!(relative_repo(&PathBuf::from(r"\tmp\repo")).is_err());
        assert!(relative_repo(&PathBuf::from(r"C:\tmp\repo")).is_err());
        assert!(relative_repo(&PathBuf::from(r"C:tmp\repo")).is_err());
    }
}

#[test]
fn rejects_colliding_repository_labels() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "one");
    let second = repository(temp.path(), "two");
    let mut context = context(temp.path(), &[("apps/api", first), ("apps-api", second)]);
    context.repositories[0].label = "apps-api".into();
    let error =
        operations::create(&context, "feature", None, Some("main"), true, &ProcessGit).unwrap_err();
    assert!(error.to_string().contains("worktree") || error.to_string().contains("collide"));
}

#[test]
fn refuses_removing_an_arbitrary_directory() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let context = context(temp.path(), &[("service-a", first)]);
    std::fs::create_dir_all(context.worktree_root.join("feature/service-a")).unwrap();
    let error = operations::remove(&context, "feature", false, &ProcessGit).unwrap_err();
    assert!(error.to_string().contains("not registered as a worktree"));
}

#[test]
fn leaves_unknown_entries_in_a_worktree_set_untouched() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let context = context(temp.path(), &[("service-a", first)]);
    let unknown = context.worktree_root.join("feature/unexpected");
    std::fs::create_dir_all(&unknown).unwrap();
    operations::remove(&context, "feature", false, &ProcessGit).unwrap();
    assert!(unknown.is_dir());
}

#[test]
fn refuses_reusing_an_arbitrary_directory() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let context = context(temp.path(), &[("service-a", first)]);
    std::fs::create_dir_all(context.worktree_root.join("feature/service-a")).unwrap();
    let error = operations::create(&context, "feature", None, Some("main"), false, &ProcessGit)
        .unwrap_err();
    assert!(error.to_string().contains("not a Git worktree"));
}
