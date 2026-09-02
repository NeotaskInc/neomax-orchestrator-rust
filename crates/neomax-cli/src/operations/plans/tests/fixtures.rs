use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use neomax_core::Engine;
use neomax_core::git::workspace::{
    AllocationStatus, IntegrationRequest, IntegrationWorkspace, PartRequest, PartWorkspace,
};
use neomax_core::scheduler::persistence::{
    PlanEvent, PlanRecord, PlanTransition, SupervisorLease, apply_transition,
};
use neomax_core::scheduler::runtime::{
    AdmissionController, AdmissionDecision, DispatchReceipt, DispatchRequest, RuntimeConfig,
    WorkerOutcome, WorkerRunner,
};
use neomax_core::scheduler::service::{
    PersistencePort, RecoveryPort, RecoveryStatus, RunAllSpec, WorkspacePort,
};
use neomax_core::scheduler::{Part, Plan};
use neomax_core::{Error, Result};

pub(super) fn spec(root: &Path) -> RunAllSpec {
    RunAllSpec {
        plan: plan(),
        repository: root.join("repository"),
        base: Some("main".into()),
        integration_branch: Some("neomax/batch-1".into()),
        plan_id: "batch-1".into(),
        runtime: RuntimeConfig {
            max_live: 1,
            max_stall_cycles: 2,
            max_attempts: 1,
        },
    }
}

pub(super) fn plan() -> Plan {
    Plan::from_parts(vec![Part {
        id: "one".into(),
        prompt: "work".into(),
        engine: Engine::Claude,
        model: None,
        area: Default::default(),
        depends_on: Default::default(),
        effort: None,
        ultra: false,
        opus: false,
        codex_model: None,
        kimi_model: None,
        order: 0,
        extra: Default::default(),
    }])
    .unwrap()
}

#[derive(Default)]
pub(super) struct MemoryPersistence {
    records: Mutex<BTreeMap<String, PlanRecord>>,
    events: Mutex<Vec<PlanEvent>>,
}

impl PersistencePort for MemoryPersistence {
    fn create(&self, record: &PlanRecord) -> Result<()> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| Error::Message("records lock poisoned".into()))?;
        if records
            .insert(record.plan_id.clone(), record.clone())
            .is_some()
        {
            return Err(Error::Conflict("plan already exists".into()));
        }
        Ok(())
    }

    fn load(&self, plan_id: &str) -> Result<PlanRecord> {
        self.records
            .lock()
            .map_err(|_| Error::Message("records lock poisoned".into()))?
            .get(plan_id)
            .cloned()
            .ok_or_else(|| Error::NotFound(plan_id.into()))
    }

    fn save(&self, record: &PlanRecord) -> Result<PlanRecord> {
        record.validate()?;
        self.records
            .lock()
            .map_err(|_| Error::Message("records lock poisoned".into()))?
            .insert(record.plan_id.clone(), record.clone());
        Ok(record.clone())
    }

    fn save_owned(
        &self,
        record: &PlanRecord,
        owner: &str,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| Error::Message("records lock poisoned".into()))?;
        let current = records
            .get(&record.plan_id)
            .ok_or_else(|| Error::NotFound(record.plan_id.clone()))?;
        if current.revision != record.revision {
            return Err(Error::Conflict("stale scheduler plan revision".into()));
        }
        let lease = current
            .supervisor_lease
            .as_ref()
            .filter(|lease| lease.owner == owner && lease.is_live(now))
            .ok_or_else(|| Error::Conflict("scheduler supervisor lease is not owned".into()))?;
        let mut replacement = record.clone();
        replacement.supervisor_lease = Some(lease.clone());
        replacement
            .supervisor_lease
            .as_mut()
            .expect("lease just assigned")
            .heartbeat(now, ttl_seconds)?;
        replacement.revision = current.revision + 1;
        replacement.validate()?;
        records.insert(record.plan_id.clone(), replacement.clone());
        Ok(replacement)
    }

    fn acquire_supervisor(
        &self,
        plan_id: &str,
        owner: &str,
        pid: Option<u32>,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| Error::Message("records lock poisoned".into()))?;
        let current = records
            .get_mut(plan_id)
            .ok_or_else(|| Error::NotFound(plan_id.into()))?;
        if let Some(lease) = current.supervisor_lease.as_mut() {
            if lease.is_live(now) && lease.owner != owner {
                return Err(Error::Conflict(
                    "scheduler supervisor lease is owned".into(),
                ));
            }
            if lease.owner == owner {
                lease.pid = pid.or(lease.pid);
                lease.heartbeat(now, ttl_seconds)?;
            } else {
                *lease = SupervisorLease::new(owner, pid, now, ttl_seconds)?;
            }
        } else {
            current.supervisor_lease = Some(SupervisorLease::new(owner, pid, now, ttl_seconds)?);
        }
        current.revision += 1;
        current.validate()?;
        Ok(current.clone())
    }

    fn heartbeat_supervisor(
        &self,
        plan_id: &str,
        owner: &str,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| Error::Message("records lock poisoned".into()))?;
        let current = records
            .get_mut(plan_id)
            .ok_or_else(|| Error::NotFound(plan_id.into()))?;
        let lease = current
            .supervisor_lease
            .as_mut()
            .filter(|lease| lease.owner == owner && lease.is_live(now))
            .ok_or_else(|| Error::Conflict("scheduler supervisor lease is not owned".into()))?;
        lease.heartbeat(now, ttl_seconds)?;
        current.revision += 1;
        current.validate()?;
        Ok(current.clone())
    }

    fn release_supervisor(&self, plan_id: &str, owner: &str) -> Result<PlanRecord> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| Error::Message("records lock poisoned".into()))?;
        let current = records
            .get_mut(plan_id)
            .ok_or_else(|| Error::NotFound(plan_id.into()))?;
        if current
            .supervisor_lease
            .as_ref()
            .is_some_and(|lease| lease.owner != owner)
        {
            return Err(Error::Conflict(
                "scheduler supervisor lease is not owned".into(),
            ));
        }
        current.supervisor_lease = None;
        current.revision += 1;
        current.validate()?;
        Ok(current.clone())
    }

    fn transition(&self, plan_id: &str, transition: PlanTransition) -> Result<PlanRecord> {
        let mut record = self.load(plan_id)?;
        apply_transition(&mut record, transition)?;
        self.save(&record)
    }

    fn append_event(&self, event: &PlanEvent) -> Result<()> {
        self.events
            .lock()
            .map_err(|_| Error::Message("events lock poisoned".into()))?
            .push(event.clone());
        Ok(())
    }
}

pub(super) struct FixtureWorkspace {
    pub root: PathBuf,
}

impl WorkspacePort for FixtureWorkspace {
    fn integration(&self, request: &IntegrationRequest) -> Result<IntegrationWorkspace> {
        let path = self.root.join(format!("integ-{}", request.plan_id));
        fs::create_dir_all(&path)?;
        Ok(IntegrationWorkspace {
            repository: request.repository.clone(),
            base: request.base.clone().unwrap_or_else(|| "main".into()),
            branch: request
                .integration_branch
                .clone()
                .unwrap_or_else(|| format!("neomax/{}", request.plan_id)),
            path,
            plan_id: request.plan_id.clone(),
            worktrees_root: self.root.clone(),
            status: AllocationStatus::Created,
        })
    }

    fn part(&self, request: &PartRequest) -> Result<PartWorkspace> {
        let path = self
            .root
            .join(format!("{}-{}", request.plan_id, request.part_id));
        fs::create_dir_all(&path)?;
        Ok(PartWorkspace {
            repository: request.repository.clone(),
            integration_branch: request.integration_branch.clone(),
            branch: format!("neomax/{}-{}", request.plan_id, request.part_id),
            path,
            plan_id: request.plan_id.clone(),
            part_id: request.part_id.clone(),
            worktrees_root: self.root.clone(),
            status: AllocationStatus::Created,
        })
    }
}

#[derive(Default)]
pub(super) struct FixtureRunner {
    outcomes: BTreeMap<String, VecDeque<WorkerOutcome>>,
}

impl FixtureRunner {
    pub(super) fn with_outcomes(
        values: impl IntoIterator<Item = (String, Vec<WorkerOutcome>)>,
    ) -> Self {
        Self {
            outcomes: values
                .into_iter()
                .map(|(id, values)| (id, values.into()))
                .collect(),
        }
    }
}

impl WorkerRunner for FixtureRunner {
    fn dispatch(&mut self, request: DispatchRequest) -> Result<DispatchReceipt> {
        Ok(DispatchReceipt::new(request.run_id, 10))
    }

    fn poll(&mut self, run_id: &str) -> Result<Option<WorkerOutcome>> {
        Ok(self.outcomes.get_mut(run_id).and_then(VecDeque::pop_front))
    }
}

pub(super) struct FixtureAdmission;

impl AdmissionController for FixtureAdmission {
    fn admit(&mut self, request: &DispatchRequest, _active: usize) -> AdmissionDecision {
        AdmissionDecision::Admitted {
            areas: request.areas.clone(),
        }
    }

    fn release(&mut self, _request: &DispatchRequest) {}
}

pub(super) struct FixtureRecovery;

impl RecoveryPort for FixtureRecovery {
    fn inspect(
        &mut self,
        _request: &DispatchRequest,
        _execution: &neomax_core::scheduler::PartExecution,
    ) -> Result<RecoveryStatus> {
        Ok(RecoveryStatus::Completed(WorkerOutcome::Completed {
            run_id: "batch-1-one".into(),
        }))
    }
}
