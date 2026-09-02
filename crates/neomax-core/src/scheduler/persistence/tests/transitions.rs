use super::super::{PlanStatus, PlanStore, PlanTransition};
use super::fixtures::record;

#[test]
fn transitions_update_parts_and_persist_an_atomic_terminal_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    store.create(&record("batch-1")).unwrap();
    store
        .transition(
            "batch-1",
            PlanTransition::PartRunning {
                part_id: "first".into(),
                run_id: "run-first".into(),
                branch: Some("neomax/batch-1-first".into()),
                profile: Some("account-1".into()),
                at: 101,
            },
        )
        .unwrap();
    store
        .transition(
            "batch-1",
            PlanTransition::PartDone {
                part_id: "first".into(),
                at: 102,
            },
        )
        .unwrap();
    store
        .transition(
            "batch-1",
            PlanTransition::PartRunning {
                part_id: "second".into(),
                run_id: "run-second".into(),
                branch: None,
                profile: None,
                at: 103,
            },
        )
        .unwrap();
    store
        .transition(
            "batch-1",
            PlanTransition::PartDone {
                part_id: "second".into(),
                at: 104,
            },
        )
        .unwrap();
    let final_record = store
        .transition("batch-1", PlanTransition::Done { at: 105 })
        .unwrap();
    assert_eq!(final_record.status, PlanStatus::Done);
    assert_eq!(final_record.ended_at, Some(105));
    assert!(final_record.state.finished());
    let bytes = std::fs::read(store.path("batch-1").unwrap()).unwrap();
    assert!(serde_json::from_slice::<serde_json::Value>(&bytes).is_ok());
    assert!(
        !std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
    );
}

#[test]
fn stale_saves_are_rejected_instead_of_overwriting_control_markers() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    let stale = record("batch-1");
    store.create(&stale).unwrap();
    store
        .transition(
            "batch-1",
            PlanTransition::Killed {
                error: Some("operator requested stop".into()),
                at: 101,
            },
        )
        .unwrap();
    let error = store.save(&stale).unwrap_err();
    assert!(error.to_string().contains("stale revision"));
    let restored = store.load("batch-1").unwrap();
    assert!(restored.killed);
    assert!(restored.kill_requested);
}

#[test]
fn recovery_increments_count_and_retains_historical_interruption_marker() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path());
    store.create(&record("batch-1")).unwrap();
    store
        .transition(
            "batch-1",
            PlanTransition::Interrupted {
                error: None,
                at: 101,
            },
        )
        .unwrap();
    let recovered = store
        .transition("batch-1", PlanTransition::Recover { at: 102 })
        .unwrap();
    assert_eq!(recovered.status, PlanStatus::Running);
    assert_eq!(recovered.recovery_count, 1);
    assert!(recovered.interrupted);
}
