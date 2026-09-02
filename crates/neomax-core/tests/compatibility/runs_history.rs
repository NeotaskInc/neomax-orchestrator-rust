use std::fs;

use chrono::{DateTime, Utc};
use neomax_core::Engine;
use neomax_core::runs::{
    EventStore, HistoryStore, HistorySummary, RunEvent, RunRecord, RunStatus, RunStore,
};
use serde_json::Value;

use super::support::{assert_fixture_is_sanitized, fixture_as, fixture_json, fixture_text};

#[test]
fn run_record_roundtrips_aliases_unknown_fields_and_attempt_prompt() {
    assert_fixture_is_sanitized("runs/run_record.json");
    let expected = fixture_json("runs/run_record.json");
    let record: RunRecord = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(record.id, "run-compat-001");
    assert_eq!(record.engine, Engine::Codex);
    assert_eq!(record.supervisor_pid, Some(654));
    assert_eq!(
        record.prompt_for_attempt(),
        "Continue from the saved worktree"
    );
    assert_eq!(record.session_history.len(), 2);
    assert_eq!(record.extra["future_run_field"]["preserve"], true);
    let serialized = serde_json::to_value(&record).unwrap();
    assert_eq!(serialized["pid"], 654);
    assert_eq!(serialized["_resume_session"], "session-compat-000");
    assert_eq!(serialized["future_run_field"]["preserve"], true);
    assert_eq!(serialized["status"], "running");
}

#[test]
fn unknown_run_status_and_nested_future_fields_survive_record_roundtrip() {
    assert_fixture_is_sanitized("runs/unknown_status.json");
    let record: RunRecord = fixture_as("runs/unknown_status.json");
    assert_eq!(record.status, RunStatus::Unknown);
    assert_eq!(record.extra["future_run_field"]["preserve"], true);
    assert_eq!(
        record.session_history[0].extra["future_session_field"],
        "preserve"
    );
    let serialized = serde_json::to_value(record).unwrap();
    assert_eq!(serialized["status"], "provider_review");
    assert_eq!(
        serialized["session_history"][0]["future_session_field"],
        "preserve"
    );
}

#[test]
fn run_events_roundtrip_extra_fields_and_skip_malformed_lines() {
    let expected: Value = fixture_json("runs/run_event.json");
    let event: RunEvent = serde_json::from_value(expected).unwrap();
    assert_eq!(event.status, Some(RunStatus::Limit));
    assert_eq!(event.extra["resets_at"], 1787490000);

    let temp = tempfile::tempdir().unwrap();
    let store = EventStore::new(temp.path());
    let at = DateTime::<Utc>::from_timestamp(1_787_488_123, 0).unwrap();
    store.append(&event, at).unwrap();
    fs::write(temp.path().join("broken.jsonl"), "not-json\n").unwrap();
    let events = store.read(Some("run-compat-001"), 0).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].extra["limit_window"], "weekly");
    assert!(
        EventStore::new(temp.path().join("missing"))
            .read(None, 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn run_store_persists_fixture_and_treats_malformed_state_as_non_loadable() {
    let temp = tempfile::tempdir().unwrap();
    let store = RunStore::new(temp.path());
    let record: RunRecord = fixture_as("runs/run_record.json");
    store.create(&record).unwrap();
    let loaded = store.load(&record.id).unwrap();
    assert_eq!(loaded.id, record.id);
    assert_eq!(loaded.extra["future_run_field"]["preserve"], true);

    fs::write(temp.path().join("malformed.json"), "{").unwrap();
    assert!(store.all().unwrap().iter().all(|run| run.id != "malformed"));
    assert!(
        RunStore::new(temp.path().join("missing"))
            .all()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn history_summary_fixture_keeps_account_number_and_terminal_status() {
    let summary: HistorySummary = fixture_as("runs/history_summary.json");
    assert_eq!(summary.account_number, Some(2));
    assert_eq!(summary.status, RunStatus::Done);
    assert_eq!(summary.children, 3);
    assert_eq!(summary.repo.as_deref(), Some("service-a"));
}

#[test]
fn history_archive_roundtrips_the_run_record_and_unknown_fields() {
    let temp = tempfile::tempdir().unwrap();
    let store = HistoryStore::new(
        temp.path().join("history.db"),
        temp.path().join("logs"),
        temp.path().join("history-logs"),
        temp.path().join("history-pending"),
    );
    let mut record: RunRecord = fixture_as("runs/run_record.json");
    record.status = RunStatus::Done;
    record.ended = Some(1_787_488_400);
    store.archive(&record, Some(2), 1_787_488_500).unwrap();
    let archived = store.get(&record.id).unwrap().unwrap();
    assert_eq!(archived.run.status, RunStatus::Done);
    assert_eq!(archived.run.extra["future_run_field"]["preserve"], true);
    let rows = store.list(10, Some(Engine::Codex)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].account_number, Some(2));
}

#[test]
fn legacy_python_history_database_keeps_account_markers_and_derives_projects() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("history.db");
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch(&fixture_text("runs/legacy_history.sql"))
        .unwrap();
    let store = HistoryStore::new(
        &database,
        temp.path().join("logs"),
        temp.path().join("history-logs"),
        temp.path().join("history-pending"),
    );
    let rows = store.list(10, None).unwrap();
    assert_eq!(rows.len(), 2);
    let orch = rows
        .iter()
        .find(|row| row.id == "legacy-python-orch")
        .unwrap();
    assert_eq!(orch.account, ".claude-orch");
    assert_eq!(orch.account_number, None);
    assert_eq!(orch.project.as_deref(), Some("service-a"));
    let numbered = rows
        .iter()
        .find(|row| row.id == "legacy-python-acct")
        .unwrap();
    assert_eq!(numbered.account_number, Some(12));
    assert_eq!(numbered.project.as_deref(), Some("worker-b"));
}

#[test]
fn legacy_string_account_number_is_accepted_and_reserialized_canonically() {
    let legacy = fixture_json("accounts/legacy_account_number_string.json");
    let shape = serde_json::json!({
        "id": "run-compat-001",
        "engine": legacy["provider"],
        "account": legacy["account"],
        "account_number": legacy["account_number"],
        "status": "done",
        "ultra": false,
        "opus": false,
        "children": 0,
        "attempt": 1,
        "started": 1,
        "ended": 2
    });
    let summary: HistorySummary = serde_json::from_value(shape).unwrap();
    assert_eq!(summary.account_number, Some(2));
    assert_eq!(serde_json::to_value(summary).unwrap()["account_number"], 2);
}

#[test]
fn legacy_orchestrator_account_marker_is_accepted_through_acct_no_alias() {
    let summary: HistorySummary = serde_json::from_value(serde_json::json!({
        "id": "run-compat-orch",
        "engine": "claude",
        "account": ".claude-orch",
        "acct_no": "orch",
        "status": "done",
        "ultra": false,
        "opus": false,
        "children": 0,
        "attempt": 1,
        "started": 1,
        "ended": 2
    }))
    .unwrap();
    assert_eq!(summary.account, ".claude-orch");
    assert_eq!(summary.account_number, None);
}

#[test]
fn account_number_rejects_negative_fractional_invalid_and_out_of_range_values() {
    let base = serde_json::json!({
        "id": "run-compat-001",
        "engine": "codex",
        "account": "codex-2",
        "status": "done",
        "ultra": false,
        "opus": false,
        "children": 0,
        "attempt": 1,
        "started": 1,
        "ended": 2
    });
    for value in [
        serde_json::json!(-1),
        serde_json::json!(-0.5),
        serde_json::json!(2.5),
        serde_json::json!(""),
        serde_json::json!("nope"),
        serde_json::json!(u64::from(u32::MAX) + 1),
        serde_json::json!(true),
    ] {
        let mut shape = base.clone();
        shape["account_number"] = value;
        assert!(
            serde_json::from_value::<HistorySummary>(shape).is_err(),
            "accepted invalid account_number"
        );
    }
}
