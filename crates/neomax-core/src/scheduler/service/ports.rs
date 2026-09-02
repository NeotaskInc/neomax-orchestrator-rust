use super::super::persistence::{PlanEvent, PlanRecord, PlanTransition};
use super::super::runtime::{DispatchRequest, RecoveredWorker, WorkerOutcome};
use crate::git::workspace::{IntegrationRequest, IntegrationWorkspace, PartRequest, PartWorkspace};
use crate::{scheduler::PartExecution, Result};

pub trait PersistencePort {
    fn create(&self, record: &PlanRecord) -> Result<()>;

    fn load(&self, plan_id: &str) -> Result<PlanRecord>;

    fn save(&self, record: &PlanRecord) -> Result<PlanRecord>;

    fn save_owned(
        &self,
        record: &PlanRecord,
        owner: &str,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord>;

    fn acquire_supervisor(
        &self,
        plan_id: &str,
        owner: &str,
        pid: Option<u32>,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord>;

    fn heartbeat_supervisor(
        &self,
        plan_id: &str,
        owner: &str,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord>;

    fn release_supervisor(&self, plan_id: &str, owner: &str) -> Result<PlanRecord>;

    fn transition(&self, plan_id: &str, transition: PlanTransition) -> Result<PlanRecord>;

    fn append_event(&self, event: &PlanEvent) -> Result<()>;
}

pub trait WorkspacePort {
    fn integration(&self, request: &IntegrationRequest) -> Result<IntegrationWorkspace>;

    fn part(&self, request: &PartRequest) -> Result<PartWorkspace>;
}

pub trait RecoveryPort {
    fn inspect(
        &mut self,
        request: &DispatchRequest,
        execution: &PartExecution,
    ) -> Result<RecoveryStatus>;

    fn live_handle(
        &mut self,
        _request: &DispatchRequest,
        _execution: &PartExecution,
    ) -> Result<Option<Box<dyn RecoveredWorker>>> {
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStatus {
    StillRunning,
    Completed(WorkerOutcome),
    Failed(WorkerOutcome),
}
