use rusqlite::Connection;

use crate::sessions::filters::DiscoveryContext;
use crate::sessions::opencode::{
    data_dir, database_path, discover_snapshot, discover_sqlite, read_database, read_usage,
};

#[test]
fn parses_parent_and_native_child_from_portal_snapshot() {
    let snapshot = serde_json::json!({
        "sessions": [
            {"id":"parent","cwd":"/repo","active":true,"model":{"providerID":"opencode","id":"big-pickle"},"tokens":{"in":5,"out":7}},
            {"id":"child","parent_id":"parent","cwd":"/repo","active":true,"title":"audit","tokens":{"out":3},"future":true}
        ]
    });
    let rows = discover_snapshot(&snapshot, "acct", &DiscoveryContext::new(100));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].model.as_deref(), Some("opencode/big-pickle"));
    let child = rows.iter().find(|row| row.is_child()).unwrap();
    assert_eq!(child.tokens.output, 3);
    assert_eq!(child.extra["future"], true);
}

#[test]
fn archived_snapshot_is_not_active() {
    let snapshot = serde_json::json!({
        "sessions": [{"id":"archived","cwd":"/repo","active":true,"archived":true}]
    });
    let rows = discover_snapshot(&snapshot, "acct", &DiscoveryContext::new(100));
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].active);
    assert!(rows[0].done);
}

#[test]
fn database_path_uses_the_upstream_default_and_profile_data_dirs() {
    let home = std::path::Path::new("/home/example");
    let default = home.join(".opencode");
    assert_eq!(data_dir(&default, home), home.join(".local/share/opencode"));
    assert_eq!(
        database_path(&default, home),
        home.join(".local/share/opencode/opencode.db")
    );
    let profile = home.join(".opencode-acct2");
    assert_eq!(data_dir(&profile, home), profile.join("opencode"));
    assert_eq!(
        database_path(&profile, home),
        profile.join("opencode/opencode.db")
    );
    let nested_default_name = home.join("projects/.opencode");
    assert_eq!(
        database_path(&nested_default_name, home),
        nested_default_name.join("opencode/opencode.db")
    );
}

#[test]
fn sqlite_snapshot_reads_usage_tools_and_file_line_counts() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("opencode.db");
    let connection = Connection::open(&db).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT, title TEXT, agent TEXT, model TEXT, time_created INTEGER, time_updated INTEGER, time_archived INTEGER, tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER, tokens_cache_read INTEGER, tokens_cache_write INTEGER, cost REAL); CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT); CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);",
        )
        .unwrap();
    let now = 100_000_i64;
    connection
        .execute(
            "INSERT INTO session VALUES ('p',NULL,'/repo','Build','build','{\"providerID\":\"opencode\",\"id\":\"big-pickle\"}',?,?,?,?,?,?,?,?,?)",
            rusqlite::params![
                now * 1000 - 1000,
                now * 1000,
                Option::<i64>::None,
                1,
                2,
                0,
                3,
                0,
                0.0
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO message VALUES ('m','p',?,?)",
            rusqlite::params![
                now * 1000,
                serde_json::json!({"role":"assistant","tokens":{"input":4,"output":5,"cache":{"read":6}},"time":{"completed":now*1000},"providerID":"opencode","modelID":"big-pickle"}).to_string()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO part VALUES ('part','m','p',?,?,?)",
            rusqlite::params![
                now * 1000,
                now * 1000,
                serde_json::json!({"type":"tool","tool":"edit","state":{"status":"completed","input":{"filePath":"/repo/a","oldString":"a\nb","newString":"a\nb\nc"}}}).to_string()
            ],
        )
        .unwrap();
    drop(connection);
    let usage = read_usage(&db, now - 10).unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].session_id, "p");
    assert_eq!(usage[0].tokens.output, 5);
    assert_eq!(usage[0].model.as_deref(), Some("opencode/big-pickle"));
    let database = read_database(&db, now - 10).unwrap();
    assert_eq!(database.sessions.len(), 1);
    assert_eq!(database.sessions[0].project_id, None);
    assert_eq!(database.sessions[0].agent.as_deref(), Some("build"));
    assert_eq!(database.messages.len(), 1);
    assert_eq!(database.parts.len(), 1);
    let rows = discover_sqlite(&db, "acct", &DiscoveryContext::new(now), now - 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tokens.output, 7);
    assert_eq!(rows[0].tool_calls, 1);
    assert_eq!(rows[0].files[0].adds, 3);
    assert_eq!(rows[0].files[0].dels, 2);
}
