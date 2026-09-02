use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::atomic::update_json_locked_strict;
use crate::settings::EffectiveSettings;
use crate::{Engine, Error, Result};

use super::capacity::{check_bound_capacity, check_capacity, reap};
use super::clock::{
    AdmissionClock, OwnerLiveness, SharedAdmissionClock, SharedOwnerLiveness, SystemAdmissionClock,
    SystemOwnerLiveness,
};
use super::lease::AdmissionLease;
use super::limits::AdmissionLimits;
use super::request::AdmissionRequest;
use super::schema::{AdmissionLeaseView, AdmissionState, LeaseRecord};

#[derive(Clone)]
pub struct DispatchAdmissionStore {
    path: PathBuf,
    lock_path: PathBuf,
    limits: AdmissionLimits,
    clock: SharedAdmissionClock,
    liveness: SharedOwnerLiveness,
    owner_pid: u32,
}

impl std::fmt::Debug for DispatchAdmissionStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DispatchAdmissionStore")
            .field("path", &self.path)
            .field("lock_path", &self.lock_path)
            .field("limits", &self.limits)
            .field("owner_pid", &self.owner_pid)
            .finish_non_exhaustive()
    }
}

impl PartialEq for DispatchAdmissionStore {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.lock_path == other.lock_path
            && self.limits == other.limits
            && self.owner_pid == other.owner_pid
    }
}

impl Eq for DispatchAdmissionStore {}

impl DispatchAdmissionStore {
    pub fn from_settings(
        state_root: impl Into<PathBuf>,
        settings: &EffectiveSettings,
    ) -> Result<Self> {
        Self::new(
            state_root.into().join("dispatch-admission.json"),
            AdmissionLimits::from_settings(settings),
        )
    }

    pub fn new(path: impl Into<PathBuf>, limits: AdmissionLimits) -> Result<Self> {
        Self::with_dependencies(
            path,
            limits,
            Arc::new(SystemAdmissionClock),
            Arc::new(SystemOwnerLiveness),
        )
    }

    pub fn with_dependencies(
        path: impl Into<PathBuf>,
        limits: AdmissionLimits,
        clock: Arc<dyn AdmissionClock>,
        liveness: Arc<dyn OwnerLiveness>,
    ) -> Result<Self> {
        limits.validate()?;
        let path = path.into();
        let lock_path = PathBuf::from(format!("{}.lock", path.display()));
        Ok(Self {
            path,
            lock_path,
            limits,
            clock,
            liveness,
            owner_pid: std::process::id(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn limits(&self) -> &AdmissionLimits {
        &self.limits
    }

    pub fn reserve(&self, request: AdmissionRequest) -> Result<AdmissionLease> {
        let lease_id = request.lease_id.clone();
        self.reserve_record(request)?;
        Ok(AdmissionLease {
            id: lease_id,
            store: self.clone(),
        })
    }

    pub fn ensure_reserved(&self, request: AdmissionRequest) -> Result<bool> {
        request.validate()?;
        let lease_id = request.lease_id.clone();
        let now = self.clock.now();
        let limits = self.limits.clone();
        let owner_pid = self.owner_pid;
        let clock = Arc::clone(&self.clock);
        let liveness = Arc::clone(&self.liveness);
        let mut created = false;
        update_json_locked_strict(&self.path, &self.lock_path, |state: &mut AdmissionState| {
            reap(state, now, &limits, liveness.as_ref());
            if let Some(existing) = state.leases.iter().find(|lease| lease.id == lease_id) {
                if existing.task != request.task_id
                    || request
                        .engine
                        .is_some_and(|engine| existing.engine != Some(engine))
                {
                    return Err(Error::Conflict(format!(
                        "dispatch lease {} belongs to a different task or provider",
                        request.lease_id
                    )));
                }
                return Ok(());
            }
            check_capacity(state, &request, &limits, None)?;
            state
                .leases
                .push(LeaseRecord::new(request, owner_pid, clock.now()));
            created = true;
            Ok(())
        })?;
        Ok(created)
    }

    pub fn bind(
        &self,
        lease_id: &str,
        engine: Engine,
        account: String,
        session: String,
    ) -> Result<()> {
        if account.trim().is_empty() || session.trim().is_empty() {
            return Err(Error::InvalidArgument(
                "dispatch lease account and session must be non-empty".into(),
            ));
        }
        let now = self.clock.now();
        let limits = self.limits.clone();
        let liveness = Arc::clone(&self.liveness);
        update_json_locked_strict(&self.path, &self.lock_path, |state: &mut AdmissionState| {
            reap(state, now, &limits, liveness.as_ref());
            let index = state
                .leases
                .iter()
                .position(|lease| lease.id == lease_id)
                .ok_or_else(|| Error::NotFound(format!("dispatch lease {lease_id}")))?;
            let current = &state.leases[index];
            if current.engine == Some(engine)
                && current.account.as_deref() == Some(account.as_str())
                && current.session.as_deref() == Some(session.as_str())
            {
                return Ok(());
            }
            check_bound_capacity(state, lease_id, engine, &account, &session, &limits)?;
            let lease = &mut state.leases[index];
            lease.engine = Some(engine);
            lease.account = Some(account);
            lease.session = Some(session);
            lease.last_seen_at = now;
            Ok(())
        })
        .map(|_| ())
    }

    pub fn release(&self, lease_id: &str) -> Result<bool> {
        if lease_id.is_empty() {
            return Ok(false);
        }
        let now = self.clock.now();
        let limits = self.limits.clone();
        let liveness = Arc::clone(&self.liveness);
        let mut released = false;
        update_json_locked_strict(&self.path, &self.lock_path, |state: &mut AdmissionState| {
            reap(state, now, &limits, liveness.as_ref());
            let before = state.leases.len();
            state.leases.retain(|lease| lease.id != lease_id);
            released = before != state.leases.len();
            Ok(())
        })?;
        Ok(released)
    }

    pub fn contains(&self, lease_id: &str) -> Result<bool> {
        Ok(self.snapshot()?.iter().any(|lease| lease.id == lease_id))
    }

    pub fn snapshot(&self) -> Result<Vec<AdmissionLeaseView>> {
        let now = self.clock.now();
        let limits = self.limits.clone();
        let liveness = Arc::clone(&self.liveness);
        let mut snapshot = Vec::new();
        update_json_locked_strict(&self.path, &self.lock_path, |state: &mut AdmissionState| {
            reap(state, now, &limits, liveness.as_ref());
            snapshot = state.leases.iter().map(AdmissionLeaseView::from).collect();
            Ok(())
        })?;
        Ok(snapshot)
    }

    fn reserve_record(&self, request: AdmissionRequest) -> Result<()> {
        request.validate()?;
        let now = self.clock.now();
        let limits = self.limits.clone();
        let owner_pid = self.owner_pid;
        let clock = Arc::clone(&self.clock);
        let liveness = Arc::clone(&self.liveness);
        update_json_locked_strict(&self.path, &self.lock_path, |state: &mut AdmissionState| {
            reap(state, now, &limits, liveness.as_ref());
            if state
                .leases
                .iter()
                .any(|lease| lease.id == request.lease_id)
            {
                return Err(Error::Conflict(format!(
                    "dispatch lease {} already exists",
                    request.lease_id
                )));
            }
            check_capacity(state, &request, &limits, None)?;
            state
                .leases
                .push(LeaseRecord::new(request, owner_pid, clock.now()));
            Ok(())
        })
        .map(|_| ())
    }
}
