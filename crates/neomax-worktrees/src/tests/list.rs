use tempfile::tempdir;

use super::fixtures::{context, repository};
use crate::git::ProcessGit;
use crate::operations;

#[test]
fn lists_only_known_project_repositories_in_stable_order() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    let context = context(temp.path(), &[("service-a", first), ("service-b", second)]);
    operations::create(&context, "feature", None, Some("main"), false, &ProcessGit).unwrap();
    let report = operations::list(&context, &ProcessGit).unwrap();
    assert_eq!(report.entries.len(), 2);
    assert_eq!(report.entries[0].task, "feature");
    assert_eq!(report.entries[0].repository, "service-a");
    assert_eq!(report.entries[1].repository, "service-b");
    assert!(
        report
            .entries
            .iter()
            .all(|entry| entry.branch == "samp/feature")
    );
}

#[test]
fn missing_worktree_root_lists_as_empty() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let context = context(temp.path(), &[("service-a", first)]);
    let report = operations::list(&context, &ProcessGit).unwrap();
    assert!(report.entries.is_empty());
}
