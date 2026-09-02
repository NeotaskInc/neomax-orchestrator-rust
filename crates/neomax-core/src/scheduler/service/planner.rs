use std::sync::Arc;

use serde_json::json;

use super::super::runtime::{Clock, DispatchPlanner, DispatchRequest};
use super::super::{Part, Plan};
use super::events::{event, event_with_fields};
use super::ports::{PersistencePort, WorkspacePort};
use crate::git::workspace::{IntegrationWorkspace, PartRequest};
use crate::{Error, Result};

pub struct DurableDispatchPlanner<W, P, C> {
    workspace: Arc<W>,
    persistence: Arc<P>,
    integration: IntegrationWorkspace,
    clock: C,
}

impl<W, P, C> DurableDispatchPlanner<W, P, C> {
    pub fn new(
        workspace: Arc<W>,
        persistence: Arc<P>,
        integration: IntegrationWorkspace,
        clock: C,
    ) -> Self {
        Self {
            workspace,
            persistence,
            integration,
            clock,
        }
    }

    pub fn integration(&self) -> &IntegrationWorkspace {
        &self.integration
    }
}

impl<W, P, C> DispatchPlanner for DurableDispatchPlanner<W, P, C>
where
    W: WorkspacePort,
    P: PersistencePort,
    C: Clock + Clone,
{
    fn plan(&self, plan: &Plan, part: &Part, attempt: u32) -> Result<DispatchRequest> {
        let repository = plan
            .repo
            .clone()
            .ok_or_else(|| Error::InvalidArgument("scheduler plan has no repository".into()))?;
        let request = PartRequest::new(
            repository,
            self.integration.branch.clone(),
            plan.plan_id
                .clone()
                .ok_or_else(|| Error::InvalidArgument("scheduler plan has no id".into()))?,
            part.id.clone(),
        );
        let plan_id = request.plan_id.clone();
        self.persistence.append_event(&event(
            &plan_id,
            "part_workspace_requested",
            self.clock.now(),
            Some(&part.id),
            None,
        )?)?;
        let workspace = self.workspace.part(&request)?;
        self.persistence.append_event(&event_with_fields(
            &plan_id,
            "part_workspace_ready",
            self.clock.now(),
            Some(&part.id),
            [
                ("path".into(), json!(workspace.path)),
                ("branch".into(), json!(workspace.branch)),
            ],
        )?)?;
        let mut dispatch = DispatchRequest::for_part(
            plan,
            part,
            format!("{plan_id}-{}", part.id),
            attempt,
            workspace.path,
        )?;
        dispatch.repository = Some(workspace.repository);
        dispatch.branch = Some(workspace.branch);
        dispatch.base = Some(self.integration.base.clone());
        Ok(dispatch)
    }
}
