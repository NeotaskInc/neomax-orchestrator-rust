use std::sync::atomic::Ordering;

use tempfile::tempdir;

use crate::config::Engine;

use super::super::AdmissionRequest;
use super::support::store;

#[test]
fn dead_owner_and_expired_ttl_are_reclaimed_atomically() {
    let temp = tempdir().unwrap();
    let (store, clock, liveness) = store(&temp.path().join("admission.json"), 1);
    let lease = store
        .reserve(AdmissionRequest::new("one", "task", Some(Engine::Claude)))
        .unwrap();
    std::mem::forget(lease);
    liveness.0.store(false, Ordering::SeqCst);
    let reclaimed = store
        .reserve(AdmissionRequest::new(
            "two",
            "task-two",
            Some(Engine::Claude),
        ))
        .unwrap();
    std::mem::forget(reclaimed);
    liveness.0.store(true, Ordering::SeqCst);
    clock.set(200.0);
    let second = store
        .reserve(AdmissionRequest::new(
            "three",
            "task-three",
            Some(Engine::Claude),
        ))
        .unwrap();
    assert_eq!(second.id, "three");
}

#[test]
fn lease_drop_releases_before_the_next_admission() {
    let temp = tempdir().unwrap();
    let (store, _, _) = store(&temp.path().join("admission.json"), 1);
    {
        let _lease = store
            .reserve(AdmissionRequest::new("one", "task", Some(Engine::Claude)))
            .unwrap();
        assert_eq!(store.snapshot().unwrap().len(), 1);
    }
    assert!(
        store
            .reserve(AdmissionRequest::new(
                "two",
                "task-two",
                Some(Engine::Claude),
            ))
            .is_ok()
    );
}
