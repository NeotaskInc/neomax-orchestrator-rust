use std::fs;

use neomax_core::usage::UsageLedger;
use neomax_usage_agent::{SweepMode, UsageCollector, WatchState};
use rusqlite::Connection;

mod support;

#[test]
fn reads_opencode_sqlite_without_touching_auth_tables() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    let db = paths
        .home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    fs::create_dir_all(db.parent().unwrap()).unwrap();
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE session (id TEXT, project_id TEXT, parent_id TEXT, directory TEXT, title TEXT, agent TEXT, model TEXT, time_created INTEGER, time_updated INTEGER, time_archived INTEGER, summary_additions INTEGER, summary_deletions INTEGER, summary_files INTEGER, tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER, tokens_cache_read INTEGER, tokens_cache_write INTEGER, cost REAL); CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT); CREATE TABLE part (id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT); CREATE TABLE auth (secret TEXT);",
        )
        .unwrap();
    connection
        .execute("INSERT INTO auth (secret) VALUES (?)", ["must-not-be-read"])
        .unwrap();
    connection
        .execute(
            "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?, ?, ?, ?, ?)",
            [
                "m1",
                "s1",
                "1800000000000",
                "1800000000000",
                r#"{"role":"assistant","providerID":"opencode-go","modelID":"ox-alpha","agent":"build","tokens":{"input":10,"output":7,"reasoning":2,"cache":{"read":3,"write":4}},"time":{"completed":1800000001000},"cost":0.25}"#,
            ],
        )
        .unwrap();

    let collector = UsageCollector::with_now(paths.clone(), 1_800_000_100);
    let mut state = WatchState::default();
    let report = collector.sweep(&mut state, SweepMode::Full, 0).unwrap();
    assert_eq!(report.records_emitted, 1);
    let records = UsageLedger::new(paths.state.usage_ledger)
        .read_deduplicated(0, 1_800_000_200)
        .unwrap();
    assert_eq!(records[0].model, "opencode-go/ox-alpha");
    assert_eq!(records[0].output, 7);
    assert_eq!(records[0].cost, Some(0.25));
}

#[test]
fn reads_grok_turn_completed_updates_and_classifies_limits() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    let updates = paths.home.join(".grok").join("sessions").join("s1");
    fs::create_dir_all(&updates).unwrap();
    fs::write(
        updates.join("summary.json"),
        r#"{"info":{"id":"s1"},"current_model_id":"grok-4.6"}"#,
    )
    .unwrap();
    fs::write(
        updates.join("updates.jsonl"),
        r#"{"timestamp":"2026-05-30T12:00:00Z","params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"p1","usage":{"inputTokens":100,"outputTokens":20,"cachedReadTokens":10,"cacheCreationTokens":5,"reasoningTokens":3,"modelCalls":1},"stop_reason":"rate_limit","agent_result":"429 rate limit"}}}
"#,
    )
    .unwrap();
    let collector = UsageCollector::with_now(paths.clone(), 1_800_000_100);
    let mut state = WatchState::default();
    let report = collector.sweep(&mut state, SweepMode::Full, 0).unwrap();
    assert_eq!(report.records_emitted, 1);
    let records = UsageLedger::new(paths.state.usage_ledger)
        .read_deduplicated(0, 1_800_000_200)
        .unwrap();
    assert_eq!(records[0].rate_limits, 1);
    assert_eq!(records[0].input, 85);
}
