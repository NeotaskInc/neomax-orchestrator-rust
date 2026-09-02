use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::{PlanStatus, PlanStore, PlanTransition};
use super::fixtures::record;

#[test]
fn creates_without_clobbering_and_preserves_unknown_fields_on_update() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    let mut initial = record("batch-1");
    initial.extra.insert("future_marker".into(), true.into());
    store.create(&initial).unwrap();
    assert!(store.create(&record("batch-1")).is_err());
    let updated = store
        .transition("batch-1", PlanTransition::Start { at: 101 })
        .unwrap();
    assert_eq!(updated.status, PlanStatus::Running);
    assert_eq!(
        updated.extra.get("future_marker"),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(store.all().unwrap().len(), 1);
}

#[test]
fn concurrent_creators_have_exactly_one_winner() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().to_path_buf();
    let winners = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..12 {
            let directory = &directory;
            let winners = &winners;
            scope.spawn(move || {
                if PlanStore::new(directory).create(&record("same")).is_ok() {
                    winners.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });
    assert_eq!(winners.load(Ordering::Relaxed), 1);
    assert_eq!(PlanStore::new(directory).all().unwrap().len(), 1);
}

#[test]
fn concurrent_locked_updates_keep_each_transition() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    store.create(&record("batch-1")).unwrap();
    std::thread::scope(|scope| {
        for index in 0..10 {
            let store = &store;
            scope.spawn(move || {
                store
                    .update("batch-1", |current| {
                        current
                            .extra
                            .insert(format!("worker-{index}"), index.into());
                        current.updated_at = 101 + index;
                        Ok(())
                    })
                    .unwrap();
            });
        }
    });
    let current = store.load("batch-1").unwrap();
    assert_eq!(current.extra.len(), 10);
    assert!((101..=110).contains(&current.updated_at));
}

#[test]
fn malformed_plan_state_fails_visibly_and_is_not_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    store.create(&record("broken")).unwrap();
    let path = store.path("broken").unwrap();
    fs::write(&path, b"{").unwrap();
    assert!(store.load("broken").is_err());
    assert!(
        store
            .transition("broken", PlanTransition::Start { at: 101 })
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), b"{");
}

#[test]
fn invalid_plan_ids_cannot_escape_the_plans_directory() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    for id in ["../escape", "nested/id", "", ".", ".."] {
        assert!(store.path(id).is_err(), "accepted plan id {id:?}");
    }
    assert!(!temp.path().join("escape.json").exists());
}

#[test]
fn all_skips_malformed_records_and_returns_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    store.create(&record("good")).unwrap();
    fs::write(temp.path().join("bad.json"), b"not-json").unwrap();
    let view = store.all_with_diagnostics().unwrap();
    assert_eq!(view.records.len(), 1);
    assert_eq!(view.diagnostics.len(), 1);
    assert!(view.diagnostics[0].path.ends_with("bad.json"));
}
