use std::fs;

use super::super::{
    branch_exists, GitWorkspaceAllocator, GitWorkspaceCleanup, IntegrationCleanup, PartCleanup,
    PartRequest,
};
use super::fixtures::{git, integration_request, repository};

#[test]
fn cleanup_keeps_integration_branch_and_retains_dirty_worktrees() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, _) = repository(temp.path());
    let allocator = GitWorkspaceAllocator::new(temp.path().join("worktrees"));
    let integration = allocator
        .integration(&integration_request(&repo, "plan-4"))
        .unwrap();
    let cleanup = GitWorkspaceCleanup;
    assert_eq!(
        cleanup.remove_clean_integration(&integration).unwrap(),
        IntegrationCleanup::Removed
    );
    assert!(!integration.path.exists());
    assert!(branch_exists(&repo, &integration.branch).unwrap());

    let dirty = allocator
        .integration(&integration_request(&repo, "plan-5"))
        .unwrap();
    fs::write(dirty.path.join("dirty.txt"), "keep\n").unwrap();
    assert_eq!(
        cleanup.remove_clean_integration(&dirty).unwrap(),
        IntegrationCleanup::RetainedDirty
    );
    assert!(dirty.path.exists());
    assert!(branch_exists(&repo, &dirty.branch).unwrap());
}

#[test]
fn part_cleanup_removes_only_clean_owned_part_and_branch() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, _) = repository(temp.path());
    let allocator = GitWorkspaceAllocator::new(temp.path().join("worktrees"));
    let integration = allocator
        .integration(&integration_request(&repo, "plan-6"))
        .unwrap();
    let part = allocator
        .part(&PartRequest::new(
            &repo,
            integration.branch.clone(),
            "plan-6",
            "part-a",
        ))
        .unwrap();
    assert_eq!(
        GitWorkspaceCleanup.remove_clean_part(&part).unwrap(),
        PartCleanup::Removed
    );
    assert!(!part.path.exists());
    assert!(!branch_exists(&repo, &part.branch).unwrap());
}

#[test]
fn part_cleanup_retains_committed_unmerged_work() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, _) = repository(temp.path());
    let allocator = GitWorkspaceAllocator::new(temp.path().join("worktrees"));
    let integration = allocator
        .integration(&integration_request(&repo, "plan-8"))
        .unwrap();
    let part = allocator
        .part(&PartRequest::new(
            &repo,
            integration.branch.clone(),
            "plan-8",
            "part-a",
        ))
        .unwrap();
    std::fs::write(part.path.join("committed.txt"), "keep\n").unwrap();
    git(&part.path, &["add", "committed.txt"]);
    git(&part.path, &["commit", "-qm", "part work"]);
    assert_eq!(
        GitWorkspaceCleanup.remove_clean_part(&part).unwrap(),
        PartCleanup::RetainedChanges
    );
    assert!(part.path.exists());
    assert!(branch_exists(&repo, &part.branch).unwrap());
}
