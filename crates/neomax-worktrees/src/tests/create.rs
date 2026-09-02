use tempfile::tempdir;

use super::fixtures::{context, repository, run};
use crate::git::ProcessGit;
use crate::operations;

#[test]
fn creates_coordinated_worktrees_across_two_repositories() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    let context = context(
        temp.path(),
        &[("service-a", first.clone()), ("service-b", second.clone())],
    );
    let report =
        operations::create(&context, "feature", None, Some("main"), false, &ProcessGit).unwrap();
    assert_eq!(report.created.len(), 2);
    for spec in &report.created {
        assert!(spec.path.is_dir());
        assert_eq!(
            crate::git::checked_text(
                &ProcessGit,
                &spec.path,
                &crate::git::args(["branch", "--show-current"])
            )
            .unwrap(),
            "samp/feature"
        );
    }
    assert!(report.plan.set_path.is_dir());
    assert!(report.plan.set_path.join("service-a").is_dir());
    assert!(report.plan.set_path.join("service-b").is_dir());
    run(&ProcessGit, &first, &["worktree", "prune"]);
    run(&ProcessGit, &second, &["worktree", "prune"]);
}

#[test]
fn dry_run_does_not_create_directories_or_branches() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let context = context(temp.path(), &[("service-a", first.clone())]);
    let report =
        operations::create(&context, "feature", None, Some("main"), true, &ProcessGit).unwrap();
    assert_eq!(report.created.len(), 1);
    assert!(!report.plan.set_path.exists());
    assert!(
        !crate::git::checked_text(
            &ProcessGit,
            &first,
            &crate::git::args(["rev-parse", "--verify", "samp/feature"])
        )
        .is_ok()
    );
}

#[test]
fn a_second_create_reuses_existing_worktrees() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let context = context(temp.path(), &[("service-a", first)]);
    operations::create(&context, "feature", None, Some("main"), false, &ProcessGit).unwrap();
    let report =
        operations::create(&context, "feature", None, Some("main"), true, &ProcessGit).unwrap();
    assert_eq!(report.reused.len(), 1);
    assert!(report.created.is_empty());
}
