use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{Engine, Result};

use super::filters::{apply_context, DiscoveryContext};
use super::types::{SessionKind, SessionRecord};

mod common;
mod extraction;
mod schema;
mod sqlite;

pub use extraction::{extract_usage, parse_message, read_usage, OpenCodeUsageRecord};
pub use schema::{
    OpenCodeDatabase, OpenCodeMessage, OpenCodePart, OpenCodeSessionRow, MESSAGE_COLUMNS,
    MESSAGE_DISCOVERY_QUERY, MESSAGE_QUERY, PART_COLUMNS, PART_QUERY, SESSION_COLUMNS,
    SESSION_DISCOVERY_QUERY, SESSION_QUERY,
};
pub use sqlite::{
    data_dir, data_dir_for_environment, database_path, database_path_for_environment,
    discover as discover_sqlite, read_database, read_messages, read_parts, read_sessions,
};

pub fn discover_snapshot(
    snapshot: &Value,
    account: &str,
    context: &DiscoveryContext,
) -> Vec<SessionRecord> {
    let mut rows = parse_snapshot(snapshot, account, context);
    rows.sort_by_key(|record| std::cmp::Reverse(record.last_active.unwrap_or_default()));
    rows
}

pub fn parse_snapshot(
    snapshot: &Value,
    account: &str,
    context: &DiscoveryContext,
) -> Vec<SessionRecord> {
    let sessions = snapshot
        .get("sessions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    let mut rows = Vec::new();
    for value in sessions {
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            continue;
        };
        let parent_id = value
            .get("parent_id")
            .and_then(Value::as_str)
            .map(Into::into);
        let mut record = SessionRecord::with_identity(id, Engine::Opencode, account);
        record.kind = if parent_id.is_some() {
            SessionKind::NativeSubagent
        } else {
            SessionKind::Main
        };
        record.parent_id = parent_id;
        record.cwd = value
            .get("cwd")
            .or_else(|| value.get("directory"))
            .and_then(Value::as_str)
            .map(PathBuf::from);
        record.project = value.get("project").and_then(Value::as_str).map(Into::into);
        record.model = value
            .get("model")
            .and_then(common::model_string)
            .or_else(|| Some(crate::providers::catalog::OPENCODE_DEFAULT_MODEL.into()));
        record.label = value.get("title").and_then(Value::as_str).map(Into::into);
        record.started = common::epoch(value.get("started").or_else(|| value.get("time_created")));
        record.last_active = common::epoch(
            value
                .get("last_active")
                .or_else(|| value.get("time_updated")),
        );
        record.archived = value
            .get("archived")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        record.active = !record.archived
            && value
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        record.working = record.active;
        record.done = record.archived
            || (!record.active
                && value
                    .get("completions")
                    .and_then(Value::as_u64)
                    .is_some_and(|count| count > 0)
                && value
                    .get("errors")
                    .and_then(Value::as_u64)
                    .unwrap_or_default()
                    == 0);
        record.tokens = common::tokens(value.get("tokens"));
        record.requests = common::integer(value, &["requests"]);
        record.completions = common::integer(value, &["completions"]);
        record.errors = common::integer(value, &["errors"]);
        record.rate_limits = common::integer(value, &["rate_limits"]);
        record.tool_calls = common::integer(value, &["tool_calls"]);
        record.tool_errors = common::integer(value, &["tool_errors"]);
        record.files = common::files(value.get("files"));
        record.extra = value.as_object().map_or_else(BTreeMap::new, |object| {
            super::artifacts::flatten_extra(
                object,
                &[
                    "id",
                    "parent_id",
                    "project_id",
                    "cwd",
                    "directory",
                    "project",
                    "title",
                    "agent",
                    "model",
                    "started",
                    "time_created",
                    "last_active",
                    "time_updated",
                    "archived",
                    "active",
                    "tokens",
                    "requests",
                    "completions",
                    "errors",
                    "rate_limits",
                    "tool_calls",
                    "tool_errors",
                    "files",
                    "cost",
                ],
            )
        });
        if !apply_context(&mut record, context).unwrap_or(false) {
            continue;
        }
        rows.push(record);
    }
    rows
}

pub fn database_records(
    db: &Path,
    account: &str,
    context: &DiscoveryContext,
    cutoff: i64,
) -> Result<Vec<SessionRecord>> {
    discover_sqlite(db, account, context, cutoff)
}
