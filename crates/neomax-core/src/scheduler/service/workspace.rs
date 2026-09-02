use crate::git::workspace::{
    GitWorkspaceAllocator, IntegrationRequest, IntegrationWorkspace, PartRequest, PartWorkspace,
};
use crate::Result;

use super::ports::WorkspacePort;

impl WorkspacePort for GitWorkspaceAllocator {
    fn integration(&self, request: &IntegrationRequest) -> Result<IntegrationWorkspace> {
        GitWorkspaceAllocator::integration(self, request)
    }

    fn part(&self, request: &PartRequest) -> Result<PartWorkspace> {
        GitWorkspaceAllocator::part(self, request)
    }
}
