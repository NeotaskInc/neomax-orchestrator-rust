use std::collections::BTreeMap;
use std::path::PathBuf;

use rusqlite::Connection;
use serde_json::Value;

use crate::{Engine, Result};

use super::super::super::filters::{apply_context, DiscoveryContext};
use super::super::super::types::{FileActivity, SessionKind, SessionRecord, SessionTokens};
use super::super::common::{
    epoch, is_rate_limit, line_count, model_string, normalize_epoch, tokens,
};
use super::query::select_query;

pub fn discover(
    db: &std::path::Path,
    account: &str,
    context: &DiscoveryContext,
    cutoff: i64,
) -> Result<Vec<SessionRecord>> {
    if !db.is_file() {
        return Ok(Vec::new());
    }
    let connection = Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )?;
    let mut rows = load_sessions(&connection, account, cutoff)?;
    load_messages(&connection, &mut rows, context)?;
    load_parts(&connection, &mut rows)?;
    let mut records = rows
        .into_values()
        .map(|mut record| {
            if record.archived {
                record.active = false;
                record.working = false;
                record.done = true;
            }
            record
        })
        .collect::<Vec<_>>();
    records.retain_mut(|record| apply_context(record, context).unwrap_or(false));
    records.sort_by_key(|record| std::cmp::Reverse(record.last_active.unwrap_or_default()));
    Ok(records)
}

fn load_sessions(
    connection: &Connection,
    account: &str,
    cutoff: i64,
) -> Result<BTreeMap<String, SessionRecord>> {
    let query = select_query(
        connection,
        "session",
        &[
            "id",
            "project_id",
            "parent_id",
            "directory",
            "title",
            "agent",
            "model",
            "time_created",
            "time_updated",
            "time_archived",
            "summary_additions",
            "summary_deletions",
            "summary_files",
            "tokens_input",
            "tokens_output",
            "tokens_reasoning",
            "tokens_cache_read",
            "tokens_cache_write",
            "cost",
        ],
        Some("time_updated"),
    )?;
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map([], |row| {
        let id: String = row.get(0)?;
        let mut record = SessionRecord::with_identity(id, Engine::Opencode, account);
        record.project = row.get(1)?;
        record.parent_id = row.get::<_, Option<String>>(2)?;
        record.kind = if record.parent_id.is_some() {
            SessionKind::NativeSubagent
        } else {
            SessionKind::Main
        };
        record.cwd = row.get::<_, Option<String>>(3)?.map(PathBuf::from);
        record.label = row.get(4)?;
        record.model = row
            .get::<_, Option<String>>(6)?
            .and_then(|model| model_string(&Value::String(model)));
        record.started = row.get::<_, Option<i64>>(7)?.map(normalize_epoch);
        record.last_active = row.get::<_, Option<i64>>(8)?.map(normalize_epoch);
        record.archived = row
            .get::<_, Option<i64>>(9)?
            .is_some_and(|value| value != 0);
        record.tokens = SessionTokens {
            input: row.get::<_, Option<i64>>(13)?.unwrap_or_default().max(0) as u64,
            output: row.get::<_, Option<i64>>(14)?.unwrap_or_default().max(0) as u64,
            reasoning: row.get::<_, Option<i64>>(15)?.unwrap_or_default().max(0) as u64,
            cache_read: row.get::<_, Option<i64>>(16)?.unwrap_or_default().max(0) as u64,
            cache_write: row.get::<_, Option<i64>>(17)?.unwrap_or_default().max(0) as u64,
            cost: row.get::<_, Option<f64>>(18)?.unwrap_or_default(),
            ..SessionTokens::default()
        };
        Ok(record)
    })?;
    let mut result = BTreeMap::new();
    for row in rows {
        let record = row?;
        if record.last_active.unwrap_or_default() >= cutoff {
            result.insert(record.id.clone(), record);
        }
    }
    Ok(result)
}

fn load_messages(
    connection: &Connection,
    sessions: &mut BTreeMap<String, SessionRecord>,
    context: &DiscoveryContext,
) -> Result<()> {
    let query = select_query(
        connection,
        "message",
        &["id", "session_id", "time_created", "time_updated", "data"],
        Some("time_created"),
    )?;
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (_, session_id, timestamp, data) = row?;
        let Some(session) = sessions.get_mut(&session_id) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if value.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let current = tokens(value.get("tokens"));
        session.tokens.add_assign(&current);
        session.requests = session.requests.saturating_add(1);
        let error = value.get("error").is_some_and(|item| !item.is_null());
        let completed = value
            .get("time")
            .and_then(|time| time.get("completed"))
            .and_then(|value| epoch(Some(value)))
            .is_some();
        if completed && !error {
            session.completions = session.completions.saturating_add(1);
        }
        if error {
            session.errors = session.errors.saturating_add(1);
            if is_rate_limit(value.get("error")) {
                session.rate_limits = session.rate_limits.saturating_add(1);
            }
        }
        if let Some(model) = value.get("modelID").and_then(Value::as_str) {
            let provider = value
                .get("providerID")
                .and_then(Value::as_str)
                .unwrap_or("opencode");
            session.model = Some(format!("{provider}/{model}"));
        }
        let created = normalize_epoch(timestamp);
        session.last_active = Some(session.last_active.unwrap_or_default().max(created));
        if !session.archived && !completed && !error {
            session.active = context.now.saturating_sub(created) <= context.active_window;
            session.working = session.active;
        }
    }
    Ok(())
}

fn load_parts(
    connection: &Connection,
    sessions: &mut BTreeMap<String, SessionRecord>,
) -> Result<()> {
    let query = select_query(
        connection,
        "part",
        &["session_id", "time_created", "time_updated", "data"],
        Some("time_created"),
    )?;
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(3)?))
    })?;
    let mut file_stats = BTreeMap::<String, BTreeMap<String, FileActivity>>::new();
    for row in rows {
        let (session_id, data) = row?;
        let Some(session) = sessions.get_mut(&session_id) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("tool") {
            continue;
        }
        session.tool_calls = session.tool_calls.saturating_add(1);
        let state = value.get("state").unwrap_or(&Value::Null);
        if state.get("status").and_then(Value::as_str) == Some("error") {
            session.tool_errors = session.tool_errors.saturating_add(1);
        }
        let tool = value
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(tool, "edit" | "write") {
            continue;
        }
        let input = state.get("input").unwrap_or(&Value::Null);
        let Some(path) = input
            .get("filePath")
            .or_else(|| input.get("filepath"))
            .or_else(|| input.get("path"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let by_session = file_stats.entry(session_id.clone()).or_default();
        let row = by_session
            .entry(path.to_string())
            .or_insert_with(|| FileActivity {
                path: path.into(),
                ..FileActivity::default()
            });
        row.ops = row.ops.saturating_add(1);
        if tool == "edit" {
            row.adds = row.adds.saturating_add(line_count(input.get("newString")));
            row.dels = row.dels.saturating_add(line_count(input.get("oldString")));
        } else {
            row.adds = row.adds.saturating_add(line_count(input.get("content")));
        }
    }
    for (session_id, by_path) in file_stats {
        if let Some(session) = sessions.get_mut(&session_id) {
            session.files = by_path.into_values().collect();
        }
    }
    Ok(())
}
