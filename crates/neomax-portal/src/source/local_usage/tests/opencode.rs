use std::fs;

use neomax_core::config::Engine;
use neomax_core::providers::ProviderProfile;
use neomax_core::usage::ProviderUsageDetail;
use rusqlite::Connection;
use serde_json::json;

use super::super::opencode::detail;
use crate::source::FilesystemPortalSource;

#[test]
fn sqlite_fixture_publishes_models_tools_unfinished_and_errors() {
    let temp = tempfile::tempdir().unwrap();
    let profile_path = temp.path().join(".opencode-acct1");
    let data_dir = profile_path.join("opencode");
    fs::create_dir_all(&data_dir).unwrap();
    let database = data_dir.join("opencode.db");
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT, title TEXT, agent TEXT, model TEXT, time_created INTEGER, time_updated INTEGER, time_archived INTEGER, tokens_input INTEGER, tokens_output INTEGER, tokens_reasoning INTEGER, tokens_cache_read INTEGER, tokens_cache_write INTEGER, cost REAL); CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT); CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT);",
        )
        .unwrap();
    let now = 1_800_000_000_i64;
    connection
        .execute(
            "INSERT INTO session VALUES ('s1',NULL,'/repo','Build','build','{\"providerID\":\"opencode\",\"id\":\"big-pickle\"}',?,?,?,?,?,?,?,?,?)",
            rusqlite::params![
                now * 1000 - 1000,
                now * 1000,
                Option::<i64>::None,
                0,
                0,
                0,
                0,
                0,
                0.0
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO message VALUES ('m1','s1',?,?,?)",
            rusqlite::params![
                now * 1000,
                now * 1000,
                json!({"role":"assistant","agent":"build","tokens":{"input":3,"output":5,"cache":{"read":2}},"time":{"completed":now*1000},"providerID":"opencode","modelID":"big-pickle"}).to_string()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO message VALUES ('m2','s1',?,?,?)",
            rusqlite::params![
                now * 1000 + 1,
                now * 1000 + 1,
                json!({"role":"assistant","agent":"build","tokens":{"input":4,"output":6},"providerID":"opencode","modelID":"big-pickle","error":{"name":"RateLimitError","data":{"statusCode":429,"message":"slow down"}}}).to_string()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO part VALUES ('p1','m1','s1',?,?,?)",
            rusqlite::params![
                now * 1000,
                now * 1000,
                json!({"type":"tool","tool":"edit","state":{"status":"completed","input":{"filePath":"/repo/a","oldString":"a\nb","newString":"a\nb\nc"}}}).to_string()
            ],
        )
        .unwrap();
    drop(connection);
    let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
    let profile = ProviderProfile {
        engine: Engine::Opencode,
        account: "1".into(),
        path: profile_path,
        reserved: false,
    };
    let detail: ProviderUsageDetail = detail(&source, &profile, 7, now - 10);
    assert!(detail.available);
    assert_eq!(detail.models[0].model, "opencode/big-pickle");
    assert_eq!(detail.totals.metrics.requests, 2);
    assert_eq!(detail.totals.metrics.completions, 1);
    assert_eq!(detail.totals.metrics.unfinished, 0);
    assert_eq!(detail.totals.metrics.errors, 1);
    assert_eq!(detail.totals.metrics.rate_limits, 1);
    assert_eq!(detail.tool_usage[0].tool, "edit");
    assert_eq!(detail.totals.files, 1);
    assert_eq!(
        detail.last_error.as_ref().unwrap().status.as_deref(),
        Some("429")
    );
}
