use std::sync::Arc;

use uuid::Uuid;

use super::super::persistence::{
    PlanRecord, PlanStatus, PlanTransition, DEFAULT_SUPERVISOR_LEASE_SECONDS,
};
use super::super::runtime::{
    AdmissionController, Clock, RuntimeConfig, RuntimeCoordinator, WorkerRunner,
};
use super::events::append;
use super::model::RunAllService;
use super::ports::RecoveryPort;
use super::ports::{PersistencePort, WorkspacePort};
use super::recovery::rehydrate_running_parts;
use super::side_effects::{DurableDispatchPlanner, PersistentAdmission, PersistentRunner};
use crate::git::workspace::{IntegrationRequest, IntegrationWorkspace};
use crate::{Error, Result};

impl<P, W, R, A, C> RunAllService<P, W, R, A, C>
where
    P: PersistencePort,
    W: WorkspacePort,
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock + Clone,
{
    pub fn start(
        spec: super::model::RunAllSpec,
        persistence: Arc<P>,
        workspace: Arc<W>,
        runner: R,
        admission: A,
        clock: C,
    ) -> Result<Self> {
        spec.runtime.validate()?;
        let mut plan = spec.plan;
        plan.plan_id = Some(spec.plan_id.clone());
        plan.repo = Some(spec.repository.clone());
        plan.base = spec.base.clone();
        plan.integration_branch = spec.integration_branch.clone();
        plan.graph()?;
        let now = clock.now();
        let record = PlanRecord::new(&spec.plan_id, plan, None, now)?;
        persistence.create(&record)?;
        append(
            persistence.as_ref(),
            &spec.plan_id,
            "plan_created",
            now,
            None,
        )?;
        let integration_request = IntegrationRequest::new(
            spec.repository,
            spec.plan_id.clone(),
            spec.base,
            spec.integration_branch,
        );
        append(
            persistence.as_ref(),
            &spec.plan_id,
            "integration_workspace_requested",
            clock.now(),
            None,
        )?;
        let integration = workspace.integration(&integration_request)?;
        append(
            persistence.as_ref(),
            &spec.plan_id,
            "integration_workspace_ready",
            clock.now(),
            None,
        )?;
        let mut saved = persistence.load(&spec.plan_id)?;
        saved.repository = Some(integration.repository.clone());
        saved.base = Some(integration.base.clone());
        saved.integration_branch = Some(integration.branch.clone());
        saved.worktree = Some(integration.path.clone());
        saved.plan.repo = saved.repository.clone();
        saved.plan.base = saved.base.clone();
        saved.plan.integration_branch = saved.integration_branch.clone();
        persistence.save(&saved)?;
        append(
            persistence.as_ref(),
            &spec.plan_id,
            "integration_workspace_persisted",
            clock.now(),
            None,
        )?;
        append(
            persistence.as_ref(),
            &spec.plan_id,
            "plan_start_requested",
            clock.now(),
            None,
        )?;
        persistence.transition(&spec.plan_id, PlanTransition::Start { at: clock.now() })?;
        append(
            persistence.as_ref(),
            &spec.plan_id,
            "plan_started",
            clock.now(),
            None,
        )?;
        Self::from_started_record(
            persistence,
            workspace,
            runner,
            admission,
            clock,
            integration,
            spec.runtime,
        )
    }

    pub fn attach<Q>(
        plan_id: &str,
        persistence: Arc<P>,
        workspace: Arc<W>,
        runner: R,
        admission: A,
        clock: C,
        options: super::model::AttachOptions<'_, Q>,
    ) -> Result<Self>
    where
        Q: RecoveryPort,
    {
        if matches!(persistence.load(plan_id)?.status, PlanStatus::Done) {
            return Err(Error::Conflict(format!(
                "scheduler plan {plan_id} cannot be attached from done"
            )));
        }
        let mut service = Self::attach_without_recovery(
            plan_id,
            persistence,
            workspace,
            runner,
            admission,
            clock,
            options.runtime,
        )?;
        let record = service.persistence.load(plan_id)?;
        if matches!(record.status, PlanStatus::Pending | PlanStatus::Running) {
            service.ensure_supervisor()?;
            let mut record = service.persistence.load(plan_id)?;
            let planner = DurableDispatchPlanner::new(
                service.workspace.clone(),
                service.persistence.clone(),
                service.integration.clone(),
                service.clock.clone(),
            );
            rehydrate_running_parts(
                service.persistence.as_ref(),
                options.recovery,
                &planner,
                &mut service.coordinator,
                &mut record,
                service.clock.clone(),
            )?;
            service.persist_state()?;
            if let Some(error) = service.admission_errors.take() {
                return Err(Error::Message(format!(
                    "scheduler area admission durability failed: {error}"
                )));
            }
        }
        Ok(service)
    }

    /// Build a control service without acquiring the plan's supervisor lease.
    /// Interrupt is an operator action and must remain possible while another
    /// scheduler supervisor owns the lease.
    pub fn attach_for_control(
        plan_id: &str,
        persistence: Arc<P>,
        workspace: Arc<W>,
        runner: R,
        admission: A,
        clock: C,
        runtime: RuntimeConfig,
    ) -> Result<Self> {
        Self::attach_without_recovery(
            plan_id,
            persistence,
            workspace,
            runner,
            admission,
            clock,
            runtime,
        )
    }

    fn attach_without_recovery(
        plan_id: &str,
        persistence: Arc<P>,
        workspace: Arc<W>,
        runner: R,
        admission: A,
        clock: C,
        runtime: RuntimeConfig,
    ) -> Result<Self> {
        runtime.validate()?;
        let record = persistence.load(plan_id)?;
        let repository = record
            .repository
            .clone()
            .ok_or_else(|| Error::InvalidState {
                path: plan_id.into(),
                message: "plan has no repository".into(),
            })?;
        let integration_request = IntegrationRequest::new(
            repository,
            plan_id,
            record.base.clone(),
            record.integration_branch.clone(),
        );
        append(
            persistence.as_ref(),
            plan_id,
            "integration_workspace_recovery_requested",
            clock.now(),
            None,
        )?;
        let integration = workspace.integration(&integration_request)?;
        append(
            persistence.as_ref(),
            plan_id,
            "integration_workspace_recovered",
            clock.now(),
            None,
        )?;
        Self::from_started_record(
            persistence,
            workspace,
            runner,
            admission,
            clock,
            integration,
            runtime,
        )
    }

    fn from_started_record(
        persistence: Arc<P>,
        workspace: Arc<W>,
        runner: R,
        admission: A,
        clock: C,
        integration: IntegrationWorkspace,
        runtime: RuntimeConfig,
    ) -> Result<Self> {
        let supervisor_owner = format!("pid-{}-{}", std::process::id(), Uuid::new_v4().simple());
        let record = persistence.load(&integration.plan_id)?;
        let durable_runner = PersistentRunner::new(
            runner,
            persistence.clone(),
            clock.clone(),
            integration.plan_id.clone(),
        );
        let durable_admission =
            PersistentAdmission::new(admission, persistence.clone(), clock.clone());
        let admission_errors = durable_admission.errors();
        let planner = DurableDispatchPlanner::new(
            workspace.clone(),
            persistence.clone(),
            integration.clone(),
            clock.clone(),
        );
        let coordinator = RuntimeCoordinator::new(
            record.plan,
            record.state,
            durable_runner,
            durable_admission,
            clock.clone(),
            planner,
            runtime,
        )?;
        Ok(Self {
            persistence,
            workspace,
            integration,
            coordinator,
            admission_errors,
            clock,
            supervisor_owner,
        })
    }
}

pub(super) const fn supervisor_lease_seconds() -> i64 {
    DEFAULT_SUPERVISOR_LEASE_SECONDS
}
