mod allocation;
mod branch;
mod cleanup;
mod identity;
mod types;

pub use allocation::GitWorkspaceAllocator;
pub use branch::{
    branch_commit, branch_exists, generated_integration_branch, generated_part_branch,
    resolve_default_branch, validate_plan_id, validate_ref_name,
};
pub use cleanup::{GitWorkspaceCleanup, IntegrationCleanup, PartCleanup};
pub use identity::{
    repository_identity, verify_repository, verify_worktree, worktree_identity, RepositoryIdentity,
    WorktreeIdentity,
};
pub use types::{
    AllocationStatus, IntegrationRequest, IntegrationWorkspace, PartRequest, PartWorkspace,
    WorkspaceLayout,
};

#[cfg(test)]
mod tests;
