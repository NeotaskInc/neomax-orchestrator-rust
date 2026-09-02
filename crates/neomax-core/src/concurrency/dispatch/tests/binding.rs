use tempfile::tempdir;

use crate::config::Engine;

use super::super::{AdmissionLimits, AdmissionRequest, DispatchAdmissionStore};
use super::support::store;

#[test]
fn account_and_session_limits_are_checked_at_bind_time() {
    let temp = tempdir().unwrap();
    let (store, _, _) = store(&temp.path().join("admission.json"), 8);
    let first = store
        .reserve(AdmissionRequest::new("one", "task-one", None))
        .unwrap();
    first
        .bind(Engine::Claude, "/profile/one", "session-a")
        .unwrap();
    let second = store
        .reserve(AdmissionRequest::new("two", "task-two", None))
        .unwrap();
    second
        .bind(Engine::Claude, "/profile/one", "session-a")
        .unwrap();
    let third = store
        .reserve(AdmissionRequest::new("three", "task-three", None))
        .unwrap();
    assert!(
        third
            .bind(Engine::Claude, "/profile/one", "session-a")
            .is_err()
    );
    assert_eq!(store.snapshot().unwrap().len(), 3);
}

#[test]
fn session_limit_counts_distinct_sessions_while_lanes_count_runs() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("admission.json");
    let clock = std::sync::Arc::new(super::support::TestClock::new(100.0));
    let liveness = std::sync::Arc::new(super::support::TestLiveness(
        std::sync::atomic::AtomicBool::new(true),
    ));
    let limits = AdmissionLimits {
        fleet_cap: Some(8),
        task_cap: 0,
        provider_cap: Some(8),
        lanes_per_account: 8,
        sessions_per_account: 2,
        lease_ttl_seconds: 60.0,
    };
    let store = DispatchAdmissionStore::with_dependencies(path, limits, clock, liveness).unwrap();
    let mut leases = Vec::new();
    for id in ["one", "two", "three"] {
        let lease = store.reserve(AdmissionRequest::new(id, id, None)).unwrap();
        lease
            .bind(Engine::Claude, "/profile/one", "session-a")
            .unwrap();
        leases.push(lease);
    }
    let fourth = store
        .reserve(AdmissionRequest::new("four", "four", None))
        .unwrap();
    assert!(
        fourth
            .bind(Engine::Claude, "/profile/one", "session-b")
            .is_ok()
    );
    leases.push(fourth);
    let fifth = store
        .reserve(AdmissionRequest::new("five", "five", None))
        .unwrap();
    assert!(
        fifth
            .bind(Engine::Claude, "/profile/one", "session-c")
            .is_err()
    );
    drop(leases);
    drop(fifth);
}
