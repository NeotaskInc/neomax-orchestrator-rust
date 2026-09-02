use std::fs;
use std::path::PathBuf;

use crate::atomic::{
    read_json, update_existing_json_locked, with_exclusive_lock, write_json_atomic,
};
use crate::{Error, Result};

use super::record::PlanRecord;
use super::transitions::{PlanTransition, apply_transition};
use super::types::SupervisorLease;
use super::validation::{lock_path, plan_path};

pub struct PlanStore {
    directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStoreDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct PlanStoreView {
    pub records: Vec<PlanRecord>,
    pub diagnostics: Vec<PlanStoreDiagnostic>,
}

impl PlanStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn directory(&self) -> &PathBuf {
        &self.directory
    }

    pub fn create(&self, record: &PlanRecord) -> Result<()> {
        let mut record = record.clone();
        normalize_record(&mut record)?;
        let path = self.path(&record.plan_id)?;
        let lock = self.lock_path(&record.plan_id)?;
        with_exclusive_lock(&lock, || {
            if path.exists() {
                return Err(Error::Conflict(format!(
                    "scheduler plan {} already exists",
                    record.plan_id
                )));
            }
            write_json_atomic(&path, &record)
        })
    }

    pub fn load(&self, plan_id: &str) -> Result<PlanRecord> {
        let path = self.path(plan_id)?;
        let record = read_json::<PlanRecord>(&path)?;
        self.validate_loaded(record, &path)
    }

    pub fn save(&self, desired: &PlanRecord) -> Result<PlanRecord> {
        let mut desired = desired.clone();
        normalize_record(&mut desired)?;
        let path = self.path(&desired.plan_id)?;
        let lock = self.lock_path(&desired.plan_id)?;
        update_existing_json_locked(&path, &lock, |current: &mut PlanRecord| {
            normalize_record(current)?;
            ensure_revision(&desired, current)?;
            let markers = current.control_markers();
            let lease = current.supervisor_lease.clone();
            let mut replacement = desired.clone();
            replacement.preserve_control_markers(markers);
            replacement.supervisor_lease = lease;
            replacement.revision = next_revision(current.revision)?;
            replacement.validate()?;
            *current = replacement;
            Ok(())
        })
    }

    pub fn update<F>(&self, plan_id: &str, update: F) -> Result<PlanRecord>
    where
        F: FnOnce(&mut PlanRecord) -> Result<()>,
    {
        let path = self.path(plan_id)?;
        let lock = self.lock_path(plan_id)?;
        update_existing_json_locked(&path, &lock, |current: &mut PlanRecord| {
            normalize_record(current)?;
            let revision = current.revision;
            let markers = current.control_markers();
            let lease = current.supervisor_lease.clone();
            update(current)?;
            current.preserve_control_markers(markers);
            current.supervisor_lease = lease;
            current.revision = next_revision(revision)?;
            current.validate()
        })
    }

    pub fn save_owned(
        &self,
        desired: &PlanRecord,
        owner: &str,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord> {
        let mut desired = desired.clone();
        normalize_record(&mut desired)?;
        let path = self.path(&desired.plan_id)?;
        let lock = self.lock_path(&desired.plan_id)?;
        update_existing_json_locked(&path, &lock, |current: &mut PlanRecord| {
            normalize_record(current)?;
            ensure_revision(&desired, current)?;
            ensure_lease_owner(current, owner, now)?;
            let markers = current.control_markers();
            let mut replacement = desired.clone();
            replacement.preserve_control_markers(markers);
            replacement.supervisor_lease = refreshed_lease(current, now, ttl_seconds)?;
            replacement.revision = next_revision(current.revision)?;
            replacement.validate()?;
            *current = replacement;
            Ok(())
        })
    }

    pub fn acquire_supervisor(
        &self,
        plan_id: &str,
        owner: &str,
        pid: Option<u32>,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord> {
        let path = self.path(plan_id)?;
        let lock = self.lock_path(plan_id)?;
        update_existing_json_locked(&path, &lock, |current: &mut PlanRecord| {
            normalize_record(current)?;
            let lease = match current.supervisor_lease.as_mut() {
                Some(lease) if lease.is_live(now) && lease.owner != owner => {
                    return Err(Error::Conflict(format!(
                        "scheduler plan {plan_id} is supervised by another live owner"
                    )));
                }
                Some(lease) if lease.owner == owner => {
                    lease.pid = pid.or(lease.pid);
                    lease.heartbeat(now, ttl_seconds)?;
                    lease.clone()
                }
                _ => SupervisorLease::new(owner, pid, now, ttl_seconds)?,
            };
            current.supervisor_lease = Some(lease);
            current.revision = next_revision(current.revision)?;
            current.validate()
        })
    }

    pub fn heartbeat_supervisor(
        &self,
        plan_id: &str,
        owner: &str,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<PlanRecord> {
        let path = self.path(plan_id)?;
        let lock = self.lock_path(plan_id)?;
        update_existing_json_locked(&path, &lock, |current: &mut PlanRecord| {
            normalize_record(current)?;
            ensure_lease_owner(current, owner, now)?;
            let lease = current.supervisor_lease.as_mut().ok_or_else(|| {
                Error::Conflict(format!("scheduler plan {plan_id} has no supervisor lease"))
            })?;
            lease.heartbeat(now, ttl_seconds)?;
            current.revision = next_revision(current.revision)?;
            current.validate()
        })
    }

    pub fn release_supervisor(&self, plan_id: &str, owner: &str) -> Result<PlanRecord> {
        let path = self.path(plan_id)?;
        let lock = self.lock_path(plan_id)?;
        update_existing_json_locked(&path, &lock, |current: &mut PlanRecord| {
            normalize_record(current)?;
            if let Some(lease) = current.supervisor_lease.as_ref() {
                if lease.owner != owner {
                    return Err(Error::Conflict(format!(
                        "scheduler plan {plan_id} is supervised by another owner"
                    )));
                }
            }
            current.supervisor_lease = None;
            current.revision = next_revision(current.revision)?;
            current.validate()
        })
    }

    pub fn transition(&self, plan_id: &str, transition: PlanTransition) -> Result<PlanRecord> {
        self.update(plan_id, |record| apply_transition(record, transition))
    }

    pub fn all(&self) -> Result<Vec<PlanRecord>> {
        Ok(self.all_with_diagnostics()?.records)
    }

    pub fn all_with_diagnostics(&self) -> Result<PlanStoreView> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PlanStoreView::default());
            }
            Err(error) => return Err(error.into()),
        };
        let mut paths = entries
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        paths.sort();
        let mut view = PlanStoreView::default();
        for path in paths {
            match read_json::<PlanRecord>(&path)
                .and_then(|record| self.validate_loaded(record, &path))
            {
                Ok(record) => view.records.push(record),
                Err(error) => view.diagnostics.push(PlanStoreDiagnostic {
                    path,
                    message: error.to_string(),
                }),
            }
        }
        Ok(view)
    }

    pub fn path(&self, plan_id: &str) -> Result<PathBuf> {
        plan_path(&self.directory, plan_id)
    }

    pub fn lock_path(&self, plan_id: &str) -> Result<PathBuf> {
        lock_path(&self.directory, plan_id)
    }

    fn validate_loaded(&self, record: PlanRecord, path: &std::path::Path) -> Result<PlanRecord> {
        let mut record = record;
        normalize_record(&mut record).map_err(|error| Error::InvalidState {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        record.validate().map_err(|error| Error::InvalidState {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Ok(record)
    }
}

fn normalize_record(record: &mut PlanRecord) -> Result<()> {
    if record.revision == 0 {
        record.revision = 1;
    }
    record.validate()
}

fn next_revision(revision: u64) -> Result<u64> {
    revision.checked_add(1).ok_or_else(|| Error::InvalidState {
        path: "scheduler".into(),
        message: "scheduler plan revision exhausted".into(),
    })
}

fn ensure_revision(desired: &PlanRecord, current: &PlanRecord) -> Result<()> {
    if desired.revision == current.revision {
        return Ok(());
    }
    Err(Error::Conflict(format!(
        "scheduler plan {} has stale revision {} (current {})",
        desired.plan_id, desired.revision, current.revision
    )))
}

fn ensure_lease_owner<'a>(
    record: &'a PlanRecord,
    owner: &str,
    now: i64,
) -> Result<&'a SupervisorLease> {
    let lease = record.supervisor_lease.as_ref().ok_or_else(|| {
        Error::Conflict(format!(
            "scheduler plan {} has no supervisor lease",
            record.plan_id
        ))
    })?;
    if lease.owner != owner {
        return Err(Error::Conflict(format!(
            "scheduler plan {} is supervised by another owner",
            record.plan_id
        )));
    }
    if !lease.is_live(now) {
        return Err(Error::Conflict(format!(
            "scheduler plan {} supervisor lease expired",
            record.plan_id
        )));
    }
    Ok(lease)
}

fn refreshed_lease(
    record: &PlanRecord,
    now: i64,
    ttl_seconds: i64,
) -> Result<Option<SupervisorLease>> {
    let mut lease = record.supervisor_lease.clone();
    if let Some(value) = lease.as_mut() {
        value.heartbeat(now, ttl_seconds)?;
    }
    Ok(lease)
}
