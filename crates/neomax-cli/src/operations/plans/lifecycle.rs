use std::path::PathBuf;
use std::sync::Arc;

use neomax_core::accounts::SelectionPolicy;
use neomax_core::git::workspace::GitWorkspaceAllocator;
use neomax_core::providers::catalog::CatalogSnapshot;
use neomax_core::providers::runtime::ProviderRuntime;
use neomax_core::scheduler::persistence::PlanRecord;
use neomax_core::scheduler::runtime::{
    AdmissionController, Clock, RuntimeConfig, TickReport, WorkerRunner,
};
use neomax_core::scheduler::service::{
    AttachOptions, CoordinatorWorkerRunner, FilePlanPersistence, PersistencePort,
    ProviderExecution, ProviderExecutionConfig, RecoveryPort, RecoveryReport, RunAllService,
    RunAllSpec, SharedTtlSchedulerAdmission, WorkspacePort, shared_ttl_scheduler_admission,
    system_coordinator_recovery,
};
use neomax_core::{EffectiveSettings, Error, Result, StatePaths, WorkerScope};

use super::status::run_report;
use super::types::{PlanRunReport, TickSummary};

pub(crate) struct PlanLifecycle<P, W, R, A, C>
where
    P: PersistencePort,
    W: WorkspacePort,
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock + Clone,
{
    persistence: Arc<P>,
    service: RunAllService<P, W, R, A, C>,
    ticks: usize,
    last_tick: Option<TickSummary>,
}

impl<P, W, R, A, C> PlanLifecycle<P, W, R, A, C>
where
    P: PersistencePort,
    W: WorkspacePort,
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock + Clone,
{
    pub(crate) fn start(
        spec: RunAllSpec,
        persistence: Arc<P>,
        workspace: Arc<W>,
        runner: R,
        admission: A,
        clock: C,
    ) -> Result<Self> {
        let service = RunAllService::start(
            spec,
            Arc::clone(&persistence),
            workspace,
            runner,
            admission,
            clock,
        )?;
        Ok(Self {
            persistence,
            service,
            ticks: 0,
            last_tick: None,
        })
    }

    pub(crate) fn attach<Q: RecoveryPort>(
        plan_id: &str,
        persistence: Arc<P>,
        workspace: Arc<W>,
        runner: R,
        admission: A,
        clock: C,
        options: AttachOptions<'_, Q>,
    ) -> Result<Self> {
        let service = RunAllService::attach(
            plan_id,
            Arc::clone(&persistence),
            workspace,
            runner,
            admission,
            clock,
            options,
        )?;
        Ok(Self {
            persistence,
            service,
            ticks: 0,
            last_tick: None,
        })
    }

    pub(crate) fn attach_for_control(
        plan_id: &str,
        persistence: Arc<P>,
        workspace: Arc<W>,
        runner: R,
        admission: A,
        clock: C,
        runtime: RuntimeConfig,
    ) -> Result<Self> {
        let service = RunAllService::attach_for_control(
            plan_id,
            Arc::clone(&persistence),
            workspace,
            runner,
            admission,
            clock,
            runtime,
        )?;
        Ok(Self {
            persistence,
            service,
            ticks: 0,
            last_tick: None,
        })
    }

    pub(crate) fn tick(&mut self) -> Result<TickReport> {
        let report = self.service.tick()?;
        self.ticks = self.ticks.saturating_add(1);
        self.last_tick = Some(TickSummary::from(&report));
        Ok(report)
    }

    pub(crate) fn run_until_terminal(&mut self, max_ticks: usize) -> Result<PlanRunReport> {
        if max_ticks == 0 {
            return Err(Error::InvalidArgument(
                "scheduler max_ticks must be positive".into(),
            ));
        }
        while !self.service.coordinator().state().finished() && self.ticks < max_ticks {
            self.tick()?;
        }
        let record = self.current_record()?;
        if !record.state.finished() {
            return Err(Error::Conflict(format!(
                "scheduler plan did not reach a terminal state after {max_ticks} ticks"
            )));
        }
        Ok(run_report(
            &record.plan_id,
            &record,
            self.ticks,
            self.last_tick.clone(),
        ))
    }

    pub(crate) fn interrupt(&mut self, error: Option<String>) -> Result<()> {
        self.service.interrupt(error)
    }

    pub(crate) fn interrupt_with_recovery<Q: RecoveryPort>(
        &mut self,
        recovery: &mut Q,
        error: Option<String>,
    ) -> Result<()> {
        self.service.interrupt_with_recovery(recovery, error)
    }

    pub(crate) fn detach(&mut self) -> Result<()> {
        self.service.detach()
    }

    pub(crate) fn recover<Q: RecoveryPort>(&mut self, recovery: &mut Q) -> Result<RecoveryReport> {
        self.service.recover(recovery)
    }

    pub(crate) fn current_record(&self) -> Result<PlanRecord> {
        let plan_id = self
            .service
            .coordinator()
            .plan()
            .plan_id
            .as_deref()
            .ok_or_else(|| Error::InvalidState {
                path: PathBuf::from("scheduler"),
                message: "runtime plan has no id".into(),
            })?;
        self.persistence.load(plan_id)
    }

    #[cfg(test)]
    pub(crate) fn service(&self) -> &RunAllService<P, W, R, A, C> {
        &self.service
    }
}

pub(crate) trait PlanFactory {
    type Lifecycle;

    fn start(&self, spec: RunAllSpec) -> Result<Self::Lifecycle>;
    fn attach(&self, plan_id: &str, runtime: RuntimeConfig) -> Result<Self::Lifecycle>;
}

pub(crate) type ProductionPlanLifecycle = PlanLifecycle<
    FilePlanPersistence,
    GitWorkspaceAllocator,
    neomax_core::scheduler::service::ProviderWorkerRunner,
    SharedTtlSchedulerAdmission,
    neomax_core::scheduler::runtime::SystemClock,
>;

pub(crate) struct ProductionPlanFactory {
    paths: StatePaths,
    settings: Arc<EffectiveSettings>,
    scope: WorkerScope,
    provider_runtime: ProviderRuntime,
}

impl ProductionPlanFactory {
    pub(crate) fn new(
        paths: StatePaths,
        settings: EffectiveSettings,
        scope: WorkerScope,
        provider_runtime: ProviderRuntime,
    ) -> Self {
        Self {
            paths,
            settings: Arc::new(settings),
            scope,
            provider_runtime,
        }
    }

    fn persistence(&self) -> Arc<FilePlanPersistence> {
        Arc::new(FilePlanPersistence::with_event_directories(
            &self.paths.plans,
            &self.paths.scheduler_events,
            &self.paths.events,
        ))
    }

    /// Count available, worker-eligible profiles from the immutable provider
    /// catalog. This is intentionally metadata-only: deriving run-all's
    /// default capacity must not trigger provider authentication calls.
    pub(crate) fn eligible_account_count(&self) -> usize {
        eligible_account_count(&self.scope, self.provider_runtime.catalog())
    }

    fn dependencies(
        &self,
        repository: PathBuf,
        _maximum: usize,
    ) -> Result<(
        Arc<FilePlanPersistence>,
        Arc<GitWorkspaceAllocator>,
        neomax_core::scheduler::service::ProviderWorkerRunner,
        SharedTtlSchedulerAdmission,
    )> {
        let persistence = self.persistence();
        let workspace = Arc::new(GitWorkspaceAllocator::new(&self.paths.worktrees));
        let provider_config = ProviderExecutionConfig::new(
            self.provider_runtime.registry_arc(),
            Arc::clone(&self.settings),
            self.paths.clone(),
        )
        .with_scope(self.scope.clone())
        .with_selection(SelectionPolicy::from_settings(&self.settings));
        let execution = ProviderExecution::new(provider_config)?;
        let runner = CoordinatorWorkerRunner::new(Arc::new(execution));
        let admission = shared_ttl_scheduler_admission(
            self.paths.state.clone(),
            &self.paths.area_locks,
            repository,
            neomax_core::scheduler::runtime::SystemClock.now(),
            neomax_core::concurrency::dispatch::AdmissionLimits::from_settings(&self.settings),
        )?;
        Ok((persistence, workspace, runner, admission))
    }
}

fn eligible_account_count(scope: &WorkerScope, catalog: &CatalogSnapshot) -> usize {
    scope
        .engines()
        .filter_map(|engine| catalog.providers.get(&engine))
        .map(|provider| {
            provider
                .profiles
                .iter()
                .filter(|profile| {
                    provider.binary.available
                        && !profile.reserved
                        && profile.eligibility.worker_eligible
                })
                .count()
        })
        .sum()
}

impl PlanFactory for ProductionPlanFactory {
    type Lifecycle = ProductionPlanLifecycle;

    fn start(&self, spec: RunAllSpec) -> Result<Self::Lifecycle> {
        let (persistence, workspace, runner, admission) =
            self.dependencies(spec.repository.clone(), spec.runtime.max_live)?;
        PlanLifecycle::start(
            spec,
            persistence,
            workspace,
            runner,
            admission,
            neomax_core::scheduler::runtime::SystemClock,
        )
    }

    fn attach(&self, plan_id: &str, runtime: RuntimeConfig) -> Result<Self::Lifecycle> {
        let persistence = self.persistence();
        let repository =
            persistence
                .load(plan_id)?
                .repository
                .ok_or_else(|| Error::InvalidState {
                    path: PathBuf::from(plan_id),
                    message: "scheduler plan has no repository".into(),
                })?;
        let (persistence, workspace, runner, admission) =
            self.dependencies(repository, runtime.max_live)?;
        let mut recovery = system_coordinator_recovery(&self.paths.runs);
        PlanLifecycle::attach(
            plan_id,
            persistence,
            workspace,
            runner,
            admission,
            neomax_core::scheduler::runtime::SystemClock,
            AttachOptions {
                runtime,
                recovery: &mut recovery,
            },
        )
    }
}

impl ProductionPlanFactory {
    pub(crate) fn run_all_with_max_ticks(
        &self,
        spec: RunAllSpec,
        max_ticks: usize,
    ) -> Result<PlanRunReport> {
        let mut lifecycle = self.start(spec)?;
        lifecycle.run_until_terminal(max_ticks)
    }

    pub(crate) fn recover(&self, plan_id: &str, runtime: RuntimeConfig) -> Result<RecoveryReport> {
        let mut lifecycle = self.attach(plan_id, runtime)?;
        let mut recovery = system_coordinator_recovery(&self.paths.runs);
        lifecycle.recover(&mut recovery)
    }

    pub(crate) fn interrupt(
        &self,
        plan_id: &str,
        runtime: RuntimeConfig,
        error: Option<String>,
    ) -> Result<()> {
        let persistence = self.persistence();
        let repository =
            persistence
                .load(plan_id)?
                .repository
                .ok_or_else(|| Error::InvalidState {
                    path: PathBuf::from(plan_id),
                    message: "scheduler plan has no repository".into(),
                })?;
        let (persistence, workspace, runner, admission) =
            self.dependencies(repository, runtime.max_live)?;
        let mut lifecycle = PlanLifecycle::attach_for_control(
            plan_id,
            persistence,
            workspace,
            runner,
            admission,
            neomax_core::scheduler::runtime::SystemClock,
            runtime,
        )?;
        let mut recovery = system_coordinator_recovery(&self.paths.runs);
        lifecycle.interrupt_with_recovery(&mut recovery, error)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use neomax_core::providers::catalog::{
        AuthMethod, AuthStatus, BinaryStatus, ProfileEligibility, ProfileSnapshot,
        ProviderSnapshot, spec,
    };

    use super::*;

    fn profile(
        engine: neomax_core::Engine,
        account: &str,
        reserved: bool,
        worker_eligible: bool,
        managed_pool_eligible: bool,
    ) -> ProfileSnapshot {
        ProfileSnapshot {
            engine,
            account: account.into(),
            path: format!("/profiles/{account}").into(),
            reserved,
            auth: AuthStatus::Authenticated {
                methods: vec![AuthMethod::ApiKey],
            },
            eligibility: ProfileEligibility {
                credential_present: true,
                authenticated: true,
                worker_eligible,
                orchestrator_eligible: false,
                rotation_eligible: false,
                managed_pool_eligible,
            },
        }
    }

    #[test]
    fn eligible_account_count_requires_binary_and_worker_eligibility() {
        let catalog = CatalogSnapshot {
            providers: BTreeMap::from([
                (
                    neomax_core::Engine::Claude,
                    ProviderSnapshot {
                        spec: spec(neomax_core::Engine::Claude),
                        binary: BinaryStatus {
                            program: "claude".into(),
                            available: false,
                            version: None,
                        },
                        profiles: vec![profile(
                            neomax_core::Engine::Claude,
                            "missing-binary",
                            false,
                            true,
                            true,
                        )],
                        models: Vec::new(),
                    },
                ),
                (
                    neomax_core::Engine::Codex,
                    ProviderSnapshot {
                        spec: spec(neomax_core::Engine::Codex),
                        binary: BinaryStatus {
                            program: "codex".into(),
                            available: true,
                            version: None,
                        },
                        profiles: vec![
                            profile(neomax_core::Engine::Codex, "worker", false, true, false),
                            profile(neomax_core::Engine::Codex, "pool-only", false, false, true),
                            profile(neomax_core::Engine::Codex, "reserved", true, true, true),
                        ],
                        models: Vec::new(),
                    },
                ),
            ]),
        };

        assert_eq!(eligible_account_count(&WorkerScope::all(), &catalog), 1);
    }
}
