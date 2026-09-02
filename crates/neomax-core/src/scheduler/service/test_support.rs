use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::git::workspace::{
    AllocationStatus, IntegrationRequest, IntegrationWorkspace, PartRequest, PartWorkspace,
};
use crate::scheduler::persistence::{
    apply_transition, PlanEvent, PlanRecord, PlanTransition, SupervisorLease,
};
use crate::scheduler::runtime::{
    AdmissionController, AdmissionDecision, DispatchReceipt, DispatchRequest, WorkerOutcome,
    WorkerRunner,
};
use crate::scheduler::service::{PersistencePort, WorkspacePort};
use crate::scheduler::{Part, Plan};
use crate::Engine;
use crate::{Error, Result};

#[derive(Default)]
pub struct MemoryPersistence {
    pub records: Mutex<BTreeMap<String, PlanRecord>>,
    pub events: Mutex<Vec<PlanEvent>>,
}

impl PersistencePort for MemoryPersistence {
    fn create(&self, record: &PlanRecord) -> Result<()> {
        record.validate()?;
        let mut records = self
            .records
            .lock()
            .map_err(|_| Error::Message("persistence lock poisoned".into()))?;
        if records.contains_key(&record.plan_id) {
            return Err(Error::Conflict("plan already exists".into()));
        }
        records.insert(record.plan_id.clone(), record.clone());
        Ok(())
    }

    fn load(&self, plan_id: &str) -> Result<PlanRecord> {
        self.records
            .lock()
            .map_err(|_| Error::Message("persistence lock poisoned".into()))?
            .get(plan_id)
            .cloned()
            .ok_or_else(|| Error::NotFound(plan_id.into()))
    }

    fn save(&self, record: &PlanRecord) -> Result<PlanRecord> {
        record.validate()?;
        let mut records = self
            .records
            .lock()
            .map_err(|_| Error::Message("persistence lock poisoned".into()))?;
        if !records.contains_key(&record.plan_id) {
            return Err(Error::NotFound(record.plan_id.clone()));
        }
        records.insert(record.plan_id.clone(), record.clone());
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
            .map_err(|_| Error::Message("persistence lock poisoned".into()))?;
        let current = records
            .get(&record.plan_id)
            .ok_or_else(|| Error::NotFound(record.plan_id.clone()))?;
        if current.revision != record.revision {
            return Err(Error::Conflict(format!(
                "scheduler plan {} has stale revision {} (current {})",
                record.plan_id, record.revision, current.revision
            )));
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
        replacement.revision =
            current
                .revision
                .checked_add(1)
                .ok_or_else(|| Error::InvalidState {
                    path: record.plan_id.clone().into(),
                    message: "scheduler plan revision exhausted".into(),
                })?;
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
            .map_err(|_| Error::Message("persistence lock poisoned".into()))?;
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
        current.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::InvalidState {
                path: plan_id.into(),
                message: "scheduler plan revision exhausted".into(),
            })?;
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
            .map_err(|_| Error::Message("persistence lock poisoned".into()))?;
        let current = records
            .get_mut(plan_id)
            .ok_or_else(|| Error::NotFound(plan_id.into()))?;
        let lease = current
            .supervisor_lease
            .as_mut()
            .filter(|lease| lease.owner == owner && lease.is_live(now))
            .ok_or_else(|| Error::Conflict("scheduler supervisor lease is not owned".into()))?;
        lease.heartbeat(now, ttl_seconds)?;
        current.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::InvalidState {
                path: plan_id.into(),
                message: "scheduler plan revision exhausted".into(),
            })?;
        current.validate()?;
        Ok(current.clone())
    }

    fn release_supervisor(&self, plan_id: &str, owner: &str) -> Result<PlanRecord> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| Error::Message("persistence lock poisoned".into()))?;
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
        current.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::InvalidState {
                path: plan_id.into(),
                message: "scheduler plan revision exhausted".into(),
            })?;
        current.validate()?;
        Ok(current.clone())
    }

    fn transition(&self, plan_id: &str, transition: PlanTransition) -> Result<PlanRecord> {
        let mut record = self.load(plan_id)?;
        apply_transition(&mut record, transition)?;
        self.save(&record)
    }

    fn append_event(&self, event: &PlanEvent) -> Result<()> {
        event.validate()?;
        self.events
            .lock()
            .map_err(|_| Error::Message("event lock poisoned".into()))?
            .push(event.clone());
        Ok(())
    }
}

pub struct FixtureWorkspace {
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
                .unwrap_or_else(|| format!("neomax/int-{}", request.plan_id)),
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
pub struct FixtureRunner {
    pub dispatched: Vec<String>,
    pub outcomes: BTreeMap<String, VecDeque<WorkerOutcome>>,
}

impl FixtureRunner {
    pub fn with_outcomes(values: impl IntoIterator<Item = (String, Vec<WorkerOutcome>)>) -> Self {
        Self {
            dispatched: Vec::new(),
            outcomes: values
                .into_iter()
                .map(|(id, values)| (id, values.into()))
                .collect(),
        }
    }
}

impl WorkerRunner for FixtureRunner {
    fn dispatch(&mut self, request: DispatchRequest) -> Result<DispatchReceipt> {
        self.dispatched.push(request.part_id);
        Ok(DispatchReceipt::new(request.run_id, 10))
    }

    fn poll(&mut self, run_id: &str) -> Result<Option<WorkerOutcome>> {
        Ok(self.outcomes.get_mut(run_id).and_then(VecDeque::pop_front))
    }
}

#[derive(Default)]
pub struct FixtureAdmission {
    pub released: Vec<String>,
}

impl AdmissionController for FixtureAdmission {
    fn admit(&mut self, request: &DispatchRequest, _active: usize) -> AdmissionDecision {
        AdmissionDecision::Admitted {
            areas: request.areas.clone(),
        }
    }

    fn release(&mut self, request: &DispatchRequest) {
        self.released.push(request.part_id.clone());
    }
}

pub fn repository(root: &Path) -> PathBuf {
    let repository = root.join("repository");
    fs::create_dir_all(&repository).unwrap();
    repository
}

pub fn one_part_plan() -> Plan {
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
