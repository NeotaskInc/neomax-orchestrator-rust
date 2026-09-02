use std::sync::Arc;
use std::thread;

use tempfile::tempdir;

use crate::config::Engine;

use super::super::AdmissionRequest;
use super::support::store;

#[test]
fn fleet_zero_denies_without_creating_a_lease() {
    let temp = tempdir().unwrap();
    let (store, _, _) = store(&temp.path().join("admission.json"), 0);
    let error = store
        .reserve(AdmissionRequest::new("one", "task", Some(Engine::Claude)))
        .unwrap_err();
    assert!(error.to_string().contains("fleet dispatch cap"));
    assert!(store.snapshot().unwrap().is_empty());
}

#[test]
fn concurrent_reservations_never_oversubscribe_the_fleet() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("admission.json");
    let (store, _, _) = store(&path, 2);
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let leases = thread::scope(|scope| {
        let mut handles = Vec::new();
        for index in 0..8 {
            let store = store.clone();
            let barrier = barrier.clone();
            handles.push(scope.spawn(move || {
                barrier.wait();
                let request = AdmissionRequest::new(
                    format!("run-{index}"),
                    format!("task-{index}"),
                    Some(Engine::Claude),
                );
                store.reserve(request).ok()
            }));
        }
        handles
            .into_iter()
            .filter_map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(leases.len(), 2);
    assert_eq!(store.snapshot().unwrap().len(), 2);
    drop(leases);
}

#[test]
fn ensure_reserved_is_idempotent_for_the_same_task() {
    let temp = tempdir().unwrap();
    let (store, _, _) = store(&temp.path().join("admission.json"), 2);
    let request = AdmissionRequest::new("one", "task", Some(Engine::Claude));
    assert!(store.ensure_reserved(request.clone()).unwrap());
    assert!(!store.ensure_reserved(request).unwrap());
    assert_eq!(store.snapshot().unwrap().len(), 1);
}

#[test]
fn ensure_reserved_rejects_reuse_for_a_different_task() {
    let temp = tempdir().unwrap();
    let (store, _, _) = store(&temp.path().join("admission.json"), 2);
    assert!(
        store
            .ensure_reserved(AdmissionRequest::new("one", "task", Some(Engine::Claude)))
            .unwrap()
    );
    let error = store
        .ensure_reserved(AdmissionRequest::new("one", "other", Some(Engine::Claude)))
        .unwrap_err();
    assert!(error.to_string().contains("different task or provider"));
}

#[test]
fn release_and_contains_track_lease_ownership() {
    let temp = tempdir().unwrap();
    let (store, _, _) = store(&temp.path().join("admission.json"), 2);
    let lease = store
        .reserve(AdmissionRequest::new("one", "task", Some(Engine::Claude)))
        .unwrap();
    assert!(store.contains("one").unwrap());
    assert!(store.release("one").unwrap());
    assert!(!store.contains("one").unwrap());
    assert!(!store.release("one").unwrap());
    drop(lease);
}
