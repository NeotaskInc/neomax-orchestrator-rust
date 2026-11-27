//! Antigravity CLI conversations, read from agy's own summary index
//! (`antigravity-cli/conversation_summaries.db`). The database is opened
//! read-only; conversation bodies stay in agy's protobuf store.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime};
use rusqlite::{Connection, OpenFlags, Row};

use crate::{Engine, Result};

use super::activity::{ActivityInput, ActivityState, classify_activity};
use super::filters::{DiscoveryContext, apply_context};
use super::types::{SessionKind, SessionRecord};

const DATABASE: &str = "antigravity-cli/conversation_summaries.db";
const QUERY: &str = "SELECT conversation_id, title, preview, step_count, last_modified_time, \
    workspace_uris, status, agent_name, parent_conversation_id, nesting_depth, not_fully_idle, \
    killed FROM conversation_summaries";

pub fn database_path(profile: &Path) -> PathBuf {
    profile.join(DATABASE)
}

pub fn discover(
    profile: &Path,
    account: &str,
    context: &DiscoveryContext,
    cutoff: i64,
) -> Result<Vec<SessionRecord>> {
    let path = database_path(profile);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    let mut statement = connection.prepare(QUERY)?;
    let rows = statement.query_map([], |row| Ok(record(row, account, context)))?;
    let mut records = Vec::new();
    for row in rows {
        let Some(mut record) = row? else {
            continue;
        };
        if record.last_active.unwrap_or_default() < cutoff {
            continue;
        }
        if apply_context(&mut record, context).unwrap_or(false) {
            records.push(record);
        }
    }
    records.sort_by_key(|record| std::cmp::Reverse(record.last_active.unwrap_or_default()));
    Ok(records)
}

fn record(row: &Row<'_>, account: &str, context: &DiscoveryContext) -> Option<SessionRecord> {
    let id = text(row, 0).filter(|id| !id.trim().is_empty())?;
    let last_active = text(row, 4).as_deref().and_then(epoch).unwrap_or_default();
    let parent = text(row, 8).filter(|parent| !parent.trim().is_empty());
    let depth = row.get::<_, Option<i64>>(9).ok().flatten().unwrap_or(0);
    let busy = flag(row, 10);
    let killed = flag(row, 11);
    let mut record = SessionRecord::with_identity(id, Engine::Antigravity, account);
    record.kind = if parent.is_some() || depth > 0 {
        SessionKind::NativeSubagent
    } else {
        SessionKind::Main
    };
    record.parent_id = parent;
    record.label = text(row, 1)
        .filter(|title| !title.trim().is_empty())
        .or_else(|| text(row, 2).filter(|preview| !preview.trim().is_empty()));
    record.cwd = text(row, 5).as_deref().and_then(workspace);
    record.last_active = Some(last_active);
    record.active = !killed
        && classify_activity(ActivityInput {
            now: context.now,
            last_modified: last_active,
            active_window: context.active_window,
            progress: busy,
            ..ActivityInput::default()
        }) == ActivityState::Active;
    record.working = record.active;
    record.done = !record.active;
    if let Some(steps) = row.get::<_, Option<i64>>(3).ok().flatten() {
        record.extra.insert("steps".into(), steps.into());
    }
    for (key, index) in [("agent", 7), ("status", 6)] {
        if let Some(value) = text(row, index).filter(|value| !value.is_empty()) {
            record.extra.insert(key.into(), value.into());
        }
    }
    Some(record)
}

fn text(row: &Row<'_>, index: usize) -> Option<String> {
    row.get::<_, Option<String>>(index).ok().flatten()
}

fn flag(row: &Row<'_>, index: usize) -> bool {
    row.get::<_, Option<i64>>(index)
        .ok()
        .flatten()
        .is_some_and(|value| value != 0)
}

fn epoch(value: &str) -> Option<i64> {
    let value = value.trim();
    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f%:z"))
        .map(|stamp| stamp.timestamp())
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .ok()
                .map(|stamp| stamp.and_utc().timestamp())
        })
}

fn workspace(value: &str) -> Option<PathBuf> {
    let first = serde_json::from_str::<Vec<String>>(value)
        .ok()
        .and_then(|uris| uris.into_iter().next())
        .or_else(|| value.split(',').next().map(str::to_owned))?;
    let path = first.trim().strip_prefix("file://").unwrap_or(first.trim());
    (!path.is_empty()).then(|| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_main_and_native_subagent_conversations() {
        let temp = tempfile::tempdir().unwrap();
        let path = database_path(temp.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE conversation_summaries (conversation_id text, title text, preview text, \
                 step_count integer, last_modified_time datetime, workspace_uris text, status text, \
                 source text, agent_name text, parent_conversation_id text, nesting_depth integer, \
                 not_fully_idle numeric, killed numeric);
                 INSERT INTO conversation_summaries VALUES ('main', 'Ship it', '', 12, \
                 '2026-11-27 23:01:00.5+00:00', '[\"file:///work/repo\"]', 'IDLE', 'cli', '', '', 0, 0, 0);
                 INSERT INTO conversation_summaries VALUES ('child', '', 'Explore', 3, \
                 '2026-11-27 23:02:00+00:00', '[\"file:///work/repo\"]', 'RUNNING', 'cli', 'general', \
                 'main', 1, 1, 0);",
            )
            .unwrap();
        drop(connection);
        let now = epoch("2026-11-27T23:02:30Z").unwrap();
        let records = discover(temp.path(), "1", &DiscoveryContext::new(now), 0).unwrap();
        assert_eq!(records.len(), 2);
        let child = &records[0];
        assert_eq!(child.id, "child");
        assert_eq!(child.kind, SessionKind::NativeSubagent);
        assert_eq!(child.parent_id.as_deref(), Some("main"));
        assert!(child.active);
        assert_eq!(child.label.as_deref(), Some("Explore"));
        let main = &records[1];
        assert_eq!(main.cwd.as_deref(), Some(Path::new("/work/repo")));
        assert_eq!(main.label.as_deref(), Some("Ship it"));
        assert_eq!(main.extra.get("steps"), Some(&12.into()));
    }
}
