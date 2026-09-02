use std::path::PathBuf;

use super::super::super::locks::{FallbackTtlLiveness, RunStoreLiveness};
use super::super::super::runtime::{AdmissionController, AdmissionDecision, DispatchRequest};
use crate::concurrency::dispatch::{AdmissionLimits, DispatchAdmissionStore};
use crate::runs::{ProcessProbe, RunStore};
use crate::Result;

pub type TtlSchedulerAdmission =
    super::super::super::runtime::AreaLockAdmission<FallbackTtlLiveness>;
pub type SharedTtlSchedulerAdmission =
    super::super::super::runtime::SharedDispatchAdmission<FallbackTtlLiveness>;

pub struct RunStoreSchedulerAdmission<'a, P>
where
    P: ProcessProbe + Send + Sync,
{
    inner: super::super::super::runtime::AreaLockAdmission<RunStoreLiveness<'a, P>>,
}

impl<'a, P> RunStoreSchedulerAdmission<'a, P>
where
    P: ProcessProbe + Send + Sync,
{
    pub fn repository(&self) -> &std::path::Path {
        self.inner.repository()
    }
}

impl<'a, P> AdmissionController for RunStoreSchedulerAdmission<'a, P>
where
    P: ProcessProbe + Send + Sync,
{
    fn admit(&mut self, request: &DispatchRequest, active: usize) -> AdmissionDecision {
        self.inner.admit(request, active)
    }

    fn release(&mut self, request: &DispatchRequest) {
        self.inner.release(request);
    }
}

pub fn ttl_scheduler_admission(
    locks_root: impl Into<PathBuf>,
    repository: impl Into<PathBuf>,
    now: i64,
    maximum: usize,
) -> TtlSchedulerAdmission {
    super::super::super::runtime::AreaLockAdmission::new(
        locks_root,
        repository,
        FallbackTtlLiveness::new(now),
        now,
        super::super::super::runtime::Capacity::new(maximum),
    )
}

pub fn shared_ttl_scheduler_admission(
    state_root: impl Into<PathBuf>,
    locks_root: impl Into<PathBuf>,
    repository: impl Into<PathBuf>,
    now: i64,
    limits: AdmissionLimits,
) -> Result<SharedTtlSchedulerAdmission> {
    let state_root = state_root.into();
    let repository = repository.into();
    let leases = DispatchAdmissionStore::new(state_root.join("dispatch-admission.json"), limits)?;
    Ok(super::super::super::runtime::SharedDispatchAdmission::new(
        super::super::super::runtime::AreaLockAdmission::new(
            locks_root,
            repository,
            FallbackTtlLiveness::new(now),
            now,
            super::super::super::runtime::Capacity::new(usize::MAX),
        ),
        leases,
    ))
}

pub fn run_store_admission<'a, P>(
    locks_root: impl Into<PathBuf>,
    repository: impl Into<PathBuf>,
    runs: &'a RunStore,
    probe: &'a P,
    now: i64,
    maximum: usize,
) -> RunStoreSchedulerAdmission<'a, P>
where
    P: ProcessProbe + Send + Sync,
{
    RunStoreSchedulerAdmission {
        inner: super::super::super::runtime::AreaLockAdmission::new(
            locks_root,
            repository,
            RunStoreLiveness::new(runs, probe, now),
            now,
            super::super::super::runtime::Capacity::new(maximum),
        ),
    }
}
