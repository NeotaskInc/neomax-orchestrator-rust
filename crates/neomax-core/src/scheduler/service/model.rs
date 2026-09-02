use std::path::PathBuf;
use std::sync::Arc;

use super::super::runtime::{
    AdmissionController, Clock, RuntimeConfig, RuntimeCoordinator, WorkerRunner,
};
use super::super::Plan;
use super::ports::{PersistencePort, RecoveryPort, WorkspacePort};
use super::side_effects::{
    DurableDispatchPlanner, ErrorState, PersistentAdmission, PersistentRunner,
};
use crate::git::workspace::IntegrationWorkspace;

#[derive(Debug, Clone)]
pub struct RunAllSpec {
    pub plan: Plan,
    pub repository: PathBuf,
    pub base: Option<String>,
    pub integration_branch: Option<String>,
    pub plan_id: String,
    pub runtime: RuntimeConfig,
}

pub struct AttachOptions<'a, Q: RecoveryPort> {
    pub runtime: RuntimeConfig,
    pub recovery: &'a mut Q,
}

impl RunAllSpec {
    pub fn new(plan: Plan, repository: impl Into<PathBuf>, plan_id: impl Into<String>) -> Self {
        Self {
            plan,
            repository: repository.into(),
            base: None,
            integration_branch: None,
            plan_id: plan_id.into(),
            runtime: RuntimeConfig::default(),
        }
    }
}

pub type Coordinator<P, W, R, A, C> = RuntimeCoordinator<
    PersistentRunner<R, P, C>,
    PersistentAdmission<A, P, C>,
    C,
    DurableDispatchPlanner<W, P, C>,
>;

pub struct RunAllService<P, W, R, A, C> {
    pub(super) persistence: Arc<P>,
    pub(super) workspace: Arc<W>,
    pub(super) integration: IntegrationWorkspace,
    pub(super) coordinator: Coordinator<P, W, R, A, C>,
    pub(super) admission_errors: ErrorState,
    pub(super) clock: C,
    pub(super) supervisor_owner: String,
}

impl<P, W, R, A, C> RunAllService<P, W, R, A, C>
where
    P: PersistencePort,
    W: WorkspacePort,
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock + Clone,
{
    pub(super) fn coordinator_now(&self) -> i64 {
        self.clock.now()
    }

    pub fn integration(&self) -> &IntegrationWorkspace {
        &self.integration
    }

    pub fn coordinator(&self) -> &Coordinator<P, W, R, A, C> {
        &self.coordinator
    }

    pub fn coordinator_mut(&mut self) -> &mut Coordinator<P, W, R, A, C> {
        &mut self.coordinator
    }

    pub fn supervisor_owner(&self) -> &str {
        &self.supervisor_owner
    }
}
