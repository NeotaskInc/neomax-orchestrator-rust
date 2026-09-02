use std::path::PathBuf;
use std::sync::Arc;

use crate::concurrency::dispatch::{
    AdmissionLimits, DispatchAdmissionStore, SystemAdmissionClock, SystemOwnerLiveness,
};
use crate::scheduler::locks::FallbackTtlLiveness;
use crate::Engine;

use super::admission::{
    AdmissionController, AdmissionDecision, AreaLockAdmission, Capacity, SharedDispatchAdmission,
};
use super::dispatch::{DefaultDispatchPlanner, DispatchPlanner};
use super::test_support::{part, plan};

#[test]
fn fresh_area_admission_acquires_and_releases_a_lock() {
    let temp = tempfile::tempdir().unwrap();
    let plan = plan(vec![part("one", Engine::Claude, &[], &["src/core"])]);
    let request = DefaultDispatchPlanner::new(temp.path())
        .plan(&plan, plan.part("one").unwrap(), 1)
        .unwrap();
    let mut admission = AreaLockAdmission::new(
        temp.path().join("locks"),
        temp.path().join("repo"),
        FallbackTtlLiveness::new(100),
        100,
        Capacity::new(1),
    );

    assert!(admission.admit(&request, 0).admitted());
    admission.release(&request);
}

#[test]
fn area_admission_is_atomic_and_releases_owned_locks() {
    let temp = tempfile::tempdir().unwrap();
    let plan = plan(vec![part("one", Engine::Claude, &[], &["src/core"])]);
    let request = DefaultDispatchPlanner::new(temp.path())
        .plan(&plan, plan.part("one").unwrap(), 1)
        .unwrap();
    let mut admission = AreaLockAdmission::new(
        temp.path().join("locks"),
        temp.path().join("repo"),
        FallbackTtlLiveness::new(100),
        100,
        Capacity::new(2),
    );
    assert!(matches!(
        admission.admit(&request, 0),
        AdmissionDecision::Admitted { .. }
    ));

    let mut second = request.clone();
    second.run_id = "another-run".into();
    assert!(matches!(
        admission.admit(&second, 1),
        AdmissionDecision::AreaBusy { .. }
    ));
    admission.release(&request);
    assert!(admission.admit(&second, 0).admitted());
    admission.release(&second);
}

#[test]
fn capacity_is_checked_before_touching_area_locks() {
    let temp = tempfile::tempdir().unwrap();
    let plan = plan(vec![part("one", Engine::Claude, &[], &[])]);
    let request = DefaultDispatchPlanner::new(PathBuf::from("."))
        .plan(&plan, plan.part("one").unwrap(), 1)
        .unwrap();
    let mut admission = AreaLockAdmission::new(
        temp.path().join("locks"),
        temp.path().join("repo"),
        FallbackTtlLiveness::new(100),
        100,
        Capacity::new(1),
    );
    assert_eq!(
        admission.admit(&request, 1),
        AdmissionDecision::CapacityExhausted {
            active: 1,
            maximum: 1
        }
    );
}

#[test]
fn shared_admission_holds_the_file_lease_until_release() {
    let temp = tempfile::tempdir().unwrap();
    let plan = plan(vec![part("one", Engine::Claude, &[], &[])]);
    let request = DefaultDispatchPlanner::new(temp.path())
        .plan(&plan, plan.part("one").unwrap(), 1)
        .unwrap();
    let limits = AdmissionLimits {
        fleet_cap: Some(1),
        task_cap: 0,
        provider_cap: Some(1),
        lanes_per_account: 1,
        sessions_per_account: 1,
        lease_ttl_seconds: 60.0,
    };
    let leases = DispatchAdmissionStore::with_dependencies(
        temp.path().join("dispatch-admission.json"),
        limits,
        Arc::new(SystemAdmissionClock),
        Arc::new(SystemOwnerLiveness),
    )
    .unwrap();
    let mut admission = SharedDispatchAdmission::new(
        AreaLockAdmission::new(
            temp.path().join("locks"),
            temp.path().join("repo"),
            FallbackTtlLiveness::new(100),
            100,
            Capacity::new(usize::MAX),
        ),
        leases,
    );

    assert!(admission.admit(&request, 0).admitted());
    assert_eq!(admission.leases().snapshot().unwrap().len(), 1);
    assert!(matches!(
        admission.admit(&request, 0),
        AdmissionDecision::Admitted { .. }
    ));
    admission.release(&request);
    assert!(admission.leases().snapshot().unwrap().is_empty());

    admission
        .leases()
        .ensure_reserved(crate::concurrency::dispatch::AdmissionRequest::new(
            request.run_id.clone(),
            request.plan_id.clone(),
            Some(request.engine),
        ))
        .unwrap();
    admission
        .leases()
        .bind(
            &request.run_id,
            request.engine,
            "/profile/one".into(),
            request.run_id.clone(),
        )
        .unwrap();
    admission.release_after_cancel(&request);
    assert_eq!(admission.leases().snapshot().unwrap().len(), 1);
    admission.release(&request);
    assert!(admission.leases().snapshot().unwrap().is_empty());
}
