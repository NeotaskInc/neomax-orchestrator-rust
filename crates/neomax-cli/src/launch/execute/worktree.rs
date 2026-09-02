use anyhow::Result;
use neomax_core::git::workspace::{GitWorkspaceAllocator, IntegrationRequest};
use neomax_core::runs::RunRecord;

use crate::context::RuntimeContext;

pub(super) fn allocate(
    context: &RuntimeContext,
    run: &mut RunRecord,
    base: Option<&str>,
) -> Result<()> {
    let repository = neomax_core::git::repository_root(&context.cwd)?;
    let allocator = GitWorkspaceAllocator::new(&context.paths.worktrees);
    let workspace = allocator.integration(&IntegrationRequest::new(
        repository,
        run.id.clone(),
        base.map(str::to_owned),
        None,
    ))?;
    run.workdir = workspace.path.clone();
    run.repo = Some(workspace.repository);
    run.worktree = Some(workspace.path);
    run.branch = Some(workspace.branch);
    run.base = Some(workspace.base.clone());
    run.base_ref = Some(workspace.base);
    Ok(())
}
