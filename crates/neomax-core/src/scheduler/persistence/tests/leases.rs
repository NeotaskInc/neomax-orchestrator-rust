use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::{DEFAULT_SUPERVISOR_LEASE_SECONDS, PlanStore};
use super::fixtures::record;

#[test]
fn legacy_records_receive_a_revision_without_losing_unknown_fields() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    let mut value = serde_json::to_value(record("legacy")).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("revision");
    object.insert("future_marker".into(), serde_json::json!({"keep": true}));
    std::fs::write(
        store.path("legacy").unwrap(),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let loaded = store.load("legacy").unwrap();
    assert_eq!(loaded.revision, 1);
    assert_eq!(loaded.extra["future_marker"]["keep"], true);
    let updated = store.update("legacy", |record| {
        record.updated_at = 101;
        Ok(())
    });
    assert!(updated.is_ok());
    assert_eq!(store.load("legacy").unwrap().revision, 2);
    assert_eq!(
        store.load("legacy").unwrap().extra["future_marker"]["keep"],
        true
    );
}

#[test]
fn only_one_concurrent_conditional_writer_can_commit_a_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    store.create(&record("cas")).unwrap();
    let first = store.load("cas").unwrap();
    let second = store.load("cas").unwrap();
    let winners = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for (label, mut snapshot) in [("first", first), ("second", second)] {
            let store = &store;
            let winners = &winners;
            scope.spawn(move || {
                snapshot
                    .extra
                    .insert(label.into(), serde_json::Value::Bool(true));
                if store.save(&snapshot).is_ok() {
                    winners.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });
    assert_eq!(winners.load(Ordering::Relaxed), 1);
    let current = store.load("cas").unwrap();
    assert_eq!(current.revision, 2);
    assert_eq!(current.extra.len(), 1);
}

#[test]
fn supervisor_lease_is_exclusive_heartbeated_and_reclaimable_after_expiry() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    store.create(&record("lease")).unwrap();
    let acquired = store
        .acquire_supervisor("lease", "owner-a", Some(11), 100, 10)
        .unwrap();
    assert_eq!(acquired.supervisor_lease.as_ref().unwrap().owner, "owner-a");
    let conflict = store.acquire_supervisor("lease", "owner-b", Some(12), 105, 10);
    assert!(conflict.is_err());
    let heartbeated = store
        .heartbeat_supervisor("lease", "owner-a", 109, 20)
        .unwrap();
    let heartbeat = heartbeated.supervisor_lease.as_ref().unwrap();
    assert_eq!(heartbeat.heartbeat_at, 109);
    assert_eq!(heartbeat.expires_at, 129);
    let conflict = store.acquire_supervisor("lease", "owner-b", Some(12), 128, 10);
    assert!(conflict.is_err());
    let reclaimed = store
        .acquire_supervisor("lease", "owner-b", Some(12), 129, 10)
        .unwrap();
    assert_eq!(
        reclaimed.supervisor_lease.as_ref().unwrap().owner,
        "owner-b"
    );
}

#[test]
fn releasing_a_lease_allows_attach_without_waiting_for_the_ttl() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    store.create(&record("release")).unwrap();
    store
        .acquire_supervisor(
            "release",
            "owner-a",
            None,
            100,
            DEFAULT_SUPERVISOR_LEASE_SECONDS,
        )
        .unwrap();
    store.release_supervisor("release", "owner-a").unwrap();
    let attached = store
        .acquire_supervisor(
            "release",
            "owner-b",
            None,
            101,
            DEFAULT_SUPERVISOR_LEASE_SECONDS,
        )
        .unwrap();
    assert_eq!(attached.supervisor_lease.as_ref().unwrap().owner, "owner-b");
}

#[test]
fn owned_save_rejects_expired_or_wrong_supervisors() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    store.create(&record("owned")).unwrap();
    store
        .acquire_supervisor("owned", "owner-a", None, 100, 10)
        .unwrap();
    let mut stale = store.load("owned").unwrap();
    stale.updated_at = 101;
    let wrong = store.save_owned(&stale, "owner-b", 101, 10).unwrap_err();
    assert!(wrong.to_string().contains("another owner"));
    let expired = store.save_owned(&stale, "owner-a", 110, 10).unwrap_err();
    assert!(expired.to_string().contains("expired"));
}
