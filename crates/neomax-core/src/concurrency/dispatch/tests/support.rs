use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::super::{AdmissionClock, AdmissionLimits, DispatchAdmissionStore, OwnerLiveness};

pub(super) struct TestClock(AtomicU64);

impl TestClock {
    pub(super) fn new(value: f64) -> Self {
        Self(AtomicU64::new(value.to_bits()))
    }

    pub(super) fn set(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::SeqCst);
    }
}

impl AdmissionClock for TestClock {
    fn now(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::SeqCst))
    }
}

pub(super) struct TestLiveness(pub(super) AtomicBool);

impl OwnerLiveness for TestLiveness {
    fn is_live(&self, _pid: u32) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub(super) fn store(
    path: &Path,
    fleet: u32,
) -> (DispatchAdmissionStore, Arc<TestClock>, Arc<TestLiveness>) {
    let clock = Arc::new(TestClock::new(100.0));
    let liveness = Arc::new(TestLiveness(AtomicBool::new(true)));
    let limits = AdmissionLimits {
        fleet_cap: Some(fleet),
        task_cap: 0,
        provider_cap: Some(fleet.max(1)),
        lanes_per_account: 2,
        sessions_per_account: 2,
        lease_ttl_seconds: 60.0,
    };
    let store =
        DispatchAdmissionStore::with_dependencies(path, limits, clock.clone(), liveness.clone())
            .unwrap();
    (store, clock, liveness)
}
