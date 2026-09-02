use std::path::{Path, PathBuf};

use super::super::locks::{AreaLockManager, LockLiveness};
use super::dispatch::DispatchRequest;
use crate::concurrency::dispatch::{
    AdmissionRequest as SharedAdmissionRequest, DispatchAdmissionStore,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionDecision {
    Admitted { areas: Vec<String> },
    AreaBusy { areas: Vec<String> },
    CapacityExhausted { active: usize, maximum: usize },
}

impl AdmissionDecision {
    pub const fn admitted(&self) -> bool {
        matches!(self, Self::Admitted { .. })
    }
}

pub trait AdmissionController {
    fn admit(&mut self, request: &DispatchRequest, active: usize) -> AdmissionDecision;

    fn admit_recovered(&mut self, request: &DispatchRequest) -> AdmissionDecision {
        self.admit(request, 0)
    }

    fn release(&mut self, request: &DispatchRequest);

    fn release_after_cancel(&mut self, request: &DispatchRequest) {
        self.release(request);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacity {
    pub maximum: usize,
}

impl Capacity {
    pub const fn new(maximum: usize) -> Self {
        Self { maximum }
    }

    pub const fn available(self, active: usize) -> usize {
        self.maximum.saturating_sub(active)
    }
}

pub struct AreaLockAdmission<L> {
    manager: AreaLockManager<L>,
    repository: PathBuf,
    capacity: Capacity,
}

impl<L: LockLiveness> AreaLockAdmission<L> {
    pub fn new(
        locks_root: impl Into<PathBuf>,
        repository: impl Into<PathBuf>,
        liveness: L,
        now: i64,
        capacity: Capacity,
    ) -> Self {
        Self {
            manager: AreaLockManager::new(locks_root, liveness, now),
            repository: repository.into(),
            capacity,
        }
    }

    pub fn repository(&self) -> &Path {
        &self.repository
    }

    pub(crate) fn acquire_areas(&mut self, request: &DispatchRequest) -> bool {
        self.manager
            .acquire_areas(&self.repository, request.areas.iter(), &request.run_id)
    }

    pub(crate) fn release_areas(&mut self, request: &DispatchRequest) {
        self.manager
            .release_area_locks(&self.repository, &request.areas, &request.run_id);
    }
}

impl<L: LockLiveness> AdmissionController for AreaLockAdmission<L> {
    fn admit(&mut self, request: &DispatchRequest, active: usize) -> AdmissionDecision {
        if active >= self.capacity.maximum {
            return AdmissionDecision::CapacityExhausted {
                active,
                maximum: self.capacity.maximum,
            };
        }
        let areas = request.areas.clone();
        if self
            .manager
            .acquire_areas(&self.repository, areas.iter(), &request.run_id)
        {
            AdmissionDecision::Admitted { areas }
        } else {
            AdmissionDecision::AreaBusy { areas }
        }
    }

    fn release(&mut self, request: &DispatchRequest) {
        self.manager
            .release_area_locks(&self.repository, &request.areas, &request.run_id);
    }
}

pub struct SharedDispatchAdmission<L> {
    areas: AreaLockAdmission<L>,
    leases: DispatchAdmissionStore,
}

impl<L: LockLiveness> SharedDispatchAdmission<L> {
    pub fn new(areas: AreaLockAdmission<L>, leases: DispatchAdmissionStore) -> Self {
        Self { areas, leases }
    }

    pub fn leases(&self) -> &DispatchAdmissionStore {
        &self.leases
    }
}

impl<L: LockLiveness> AdmissionController for SharedDispatchAdmission<L> {
    fn admit(&mut self, request: &DispatchRequest, _active: usize) -> AdmissionDecision {
        let shared = SharedAdmissionRequest::new(
            request.run_id.clone(),
            request.plan_id.clone(),
            Some(request.engine),
        );
        if let Err(error) = self.leases.ensure_reserved(shared) {
            let message = error.to_string();
            let (active, maximum) = admission_capacity(&message, self.leases.limits());
            return AdmissionDecision::CapacityExhausted { active, maximum };
        }
        if !self.areas.acquire_areas(request) {
            let _ = self.leases.release(&request.run_id);
            return AdmissionDecision::AreaBusy {
                areas: request.areas.clone(),
            };
        }
        AdmissionDecision::Admitted {
            areas: request.areas.clone(),
        }
    }

    fn release(&mut self, request: &DispatchRequest) {
        self.areas.release_areas(request);
        let _ = self.leases.release(&request.run_id);
    }

    fn release_after_cancel(&mut self, request: &DispatchRequest) {
        self.areas.release_areas(request);
    }
}

fn admission_capacity(
    message: &str,
    limits: &crate::concurrency::dispatch::AdmissionLimits,
) -> (usize, usize) {
    let fallback = limits
        .fleet_cap
        .or(limits.provider_cap)
        .unwrap_or(usize::MAX as u32) as usize;
    message
        .split_whitespace()
        .find_map(|token| {
            let (active, maximum) = token.split_once('/')?;
            Some((active.parse().ok()?, maximum.parse().ok()?))
        })
        .unwrap_or((fallback, fallback))
}
