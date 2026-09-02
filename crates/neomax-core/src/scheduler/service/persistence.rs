use chrono::{DateTime, Utc};
use std::path::PathBuf;

use super::super::persistence::{PlanEvent, PlanEventStore, PlanRecord, PlanStore, PlanTransition};
use super::ports::PersistencePort;
use crate::Result;

pub struct FilePlanPersistence {
    plans: PlanStore,
    events: PlanEventStore,
}

impl FilePlanPersistence {
    pub fn new(plans_directory: impl Into<PathBuf>, events_directory: impl Into<PathBuf>) -> Self {
        Self {
            plans: PlanStore::new(plans_directory),
            events: PlanEventStore::new(events_directory),
        }
    }

    pub fn with_event_directories(
        plans_directory: impl Into<PathBuf>,
        scheduler_events_directory: impl Into<PathBuf>,
        legacy_events_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            plans: PlanStore::new(plans_directory),
            events: PlanEventStore::with_legacy_directory(
                scheduler_events_directory,
                legacy_events_directory,
            ),
        }
    }

    pub fn plans(&self) -> &PlanStore {
        &self.plans
    }

    pub fn events(&self) -> &PlanEventStore {
        &self.events
    }
}

impl PersistencePort for FilePlanPersistence {
    fn create(&self, record: &PlanRecord) -> Result<()> {
        self.plans.create(record)
    }

    fn load(&self, plan_id: &str) -> Result<PlanRecord> {
        self.plans.load(plan_id)
    }

    fn save(&self, record: &PlanRecord) -> Result<PlanRecord> {
        self.plans.save(record)
    }

    fn save_owned(
        &self,
        record: &PlanRecord,
        owner: &str,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord> {
        self.plans.save_owned(record, owner, now, ttl_seconds)
    }

    fn acquire_supervisor(
        &self,
        plan_id: &str,
        owner: &str,
        pid: Option<u32>,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord> {
        self.plans
            .acquire_supervisor(plan_id, owner, pid, now, ttl_seconds)
    }

    fn heartbeat_supervisor(
        &self,
        plan_id: &str,
        owner: &str,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord> {
        self.plans
            .heartbeat_supervisor(plan_id, owner, now, ttl_seconds)
    }

    fn release_supervisor(&self, plan_id: &str, owner: &str) -> Result<PlanRecord> {
        self.plans.release_supervisor(plan_id, owner)
    }

    fn transition(&self, plan_id: &str, transition: PlanTransition) -> Result<PlanRecord> {
        self.plans.transition(plan_id, transition)
    }

    fn append_event(&self, event: &PlanEvent) -> Result<()> {
        let at = DateTime::<Utc>::from_timestamp(event.ts, 0)
            .ok_or_else(|| crate::Error::InvalidArgument("invalid scheduler event time".into()))?;
        self.events.append(event, at)
    }
}
