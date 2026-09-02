use std::fs;

use super::super::{
    branch_exists, resolve_default_branch, worktree_identity, AllocationStatus,
    GitWorkspaceAllocator, PartRequest,
};
use super::fixtures::{git, integration_request, repository};

#[test]
fn resolves_default_branch_and_allocates_integration_idempotently() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, _) = repository(temp.path());
    let worktrees = temp.path().join("worktrees");
    assert_eq!(resolve_default_branch(&repo).unwrap(), "main");
    let allocator = GitWorkspaceAllocator::new(&worktrees);
    let first = allocator
        .integration(&integration_request(&repo, "plan-1"))
        .unwrap();
    assert_eq!(first.status, AllocationStatus::Created);
    assert_eq!(first.branch, "neomax/int-plan-1");
    assert_eq!(
        crate::git::current_branch(&first.path).unwrap(),
        first.branch
    );
    assert!(branch_exists(&repo, &first.branch).unwrap());

    let second = allocator
        .integration(&integration_request(&repo, "plan-1"))
        .unwrap();
    assert_eq!(second.status, AllocationStatus::Reused);
    assert_eq!(second.path, first.path);
    assert_eq!(
        worktree_identity(&second.path).unwrap().branch,
        second.branch
    );
}

#[test]
fn allocates_part_worktrees_from_the_integration_branch() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, _) = repository(temp.path());
    let allocator = GitWorkspaceAllocator::new(temp.path().join("worktrees"));
    let integration = allocator
        .integration(&integration_request(&repo, "plan-2"))
        .unwrap();
    let request = PartRequest::new(&repo, integration.branch.clone(), "plan-2", "part-a");
    let part = allocator.part(&request).unwrap();
    assert_eq!(part.status, AllocationStatus::Created);
    assert_eq!(part.branch, "neomax/plan-2-part-a");
    assert_eq!(worktree_identity(&part.path).unwrap().branch, part.branch);
    let resumed = allocator.part(&request).unwrap();
    assert_eq!(resumed.status, AllocationStatus::Reused);
}

#[test]
fn refuses_unknown_existing_paths_without_removing_them() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, _) = repository(temp.path());
    let root = temp.path().join("worktrees");
    let allocator = GitWorkspaceAllocator::new(&root);
    fs::create_dir_all(root.join("integ-plan-3")).unwrap();
    fs::write(root.join("integ-plan-3").join("marker"), "keep").unwrap();
    assert!(allocator
        .integration(&integration_request(&repo, "plan-3"))
        .is_err());
    assert!(!branch_exists(&repo, "neomax/int-plan-3").unwrap());
    assert!(root.join("integ-plan-3/marker").exists());
}

#[test]
fn rejects_a_matching_path_checked_out_on_the_wrong_branch() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, _) = repository(temp.path());
    let allocator = GitWorkspaceAllocator::new(temp.path().join("worktrees"));
    let integration = allocator
        .integration(&integration_request(&repo, "plan-7"))
        .unwrap();
    git(&repo, &["branch", "neomax/other", "main"]);
    git(&integration.path, &["checkout", "-q", "neomax/other"]);
    assert!(allocator
        .integration(&integration_request(&repo, "plan-7"))
        .is_err());
}
