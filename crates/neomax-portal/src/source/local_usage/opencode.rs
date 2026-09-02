use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use neomax_core::config::Engine;
use neomax_core::providers::ProviderProfile;
use neomax_core::sessions::opencode;
use neomax_core::usage::{
    LocalToolUsageRow, LocalUsageEntry, LocalUsageSnapshot, ProviderUsageDetail, UsageCounts,
    UsageMetrics, build_provider_usage_detail,
};
use serde_json::Value;

use super::errors::{local_error, safe_error};
use super::tools::line_count;
use crate::source::FilesystemPortalSource;

pub(crate) fn detail(
    source: &FilesystemPortalSource,
    profile: &ProviderProfile,
    days: u32,
    cutoff: i64,
) -> ProviderUsageDetail {
    debug_assert_eq!(profile.engine, Engine::Opencode);
    let database = opencode::database_path(&profile.path, &source.home);
    let metadata = fs::metadata(&database).ok();
    let mut snapshot = LocalUsageSnapshot {
        available: metadata.as_ref().is_some_and(|value| value.is_file()),
        source: "opencode.db".into(),
        database: Some(database.clone()),
        db_bytes: metadata.map(|value| value.len()),
        account: profile.account.clone(),
        window_days: days,
        ..LocalUsageSnapshot::default()
    };
    if !snapshot.available {
        return build_provider_usage_detail(snapshot);
    }
    let database = match opencode::read_database(&database, cutoff) {
        Ok(database) => database,
        Err(error) => {
            snapshot.available = false;
            snapshot.error = Some(safe_error(&error.to_string()));
            return build_provider_usage_detail(snapshot);
        }
    };
    snapshot.sessions = database.sessions.len() as u64;
    snapshot.main_sessions = database
        .sessions
        .iter()
        .filter(|session| session.parent_id.is_none())
        .count() as u64;
    snapshot.native_subagents = database
        .sessions
        .iter()
        .filter(|session| session.parent_id.is_some())
        .count() as u64;
    snapshot.last_activity = database
        .sessions
        .iter()
        .filter_map(|session| session.updated)
        .max()
        .unwrap_or_default();
    let mut files = BTreeSet::new();
    let mut file_adds = 0_u64;
    let mut file_dels = 0_u64;
    let mut tool_rows = BTreeMap::<(String, String), u64>::new();
    for part in &database.parts {
        let Some(data) = part.data.as_object() else {
            continue;
        };
        if data.get("type").and_then(Value::as_str) != Some("tool") {
            continue;
        }
        let tool = data
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let file_tool = matches!(tool.as_str(), "edit" | "write");
        let state = data.get("state").and_then(Value::as_object);
        let status = state
            .and_then(|state| state.get("status"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        *tool_rows.entry((tool.clone(), status)).or_default() += 1;
        let Some(input) = state.and_then(|state| state.get("input")) else {
            continue;
        };
        let Some(path) = input
            .get("filePath")
            .or_else(|| input.get("filepath"))
            .or_else(|| input.get("path"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if file_tool {
            files.insert(path.to_owned());
            if tool == "edit" {
                file_adds = file_adds.saturating_add(line_count(input.get("newString")));
                file_dels = file_dels.saturating_add(line_count(input.get("oldString")));
            } else {
                file_adds = file_adds.saturating_add(line_count(input.get("content")));
            }
        }
    }
    snapshot.files = files.len() as u64;
    snapshot.adds = file_adds;
    snapshot.dels = file_dels;
    snapshot.tool_usage = tool_rows
        .into_iter()
        .map(|((tool, status), calls)| LocalToolUsageRow {
            tool,
            status,
            calls,
        })
        .collect();
    snapshot.tool_calls = Some(
        snapshot
            .tool_usage
            .iter()
            .map(|row| row.calls)
            .fold(0, u64::saturating_add),
    );
    snapshot.tool_errors = Some(
        snapshot
            .tool_usage
            .iter()
            .filter(|row| row.status.eq_ignore_ascii_case("error"))
            .map(|row| row.calls)
            .fold(0, u64::saturating_add),
    );
    for message in &database.messages {
        let Some(record) = opencode::parse_message(message) else {
            continue;
        };
        let timestamp = record.timestamp.max(0);
        snapshot.last_activity = snapshot.last_activity.max(timestamp);
        let cost = record.cost.unwrap_or_default();
        let metrics = UsageMetrics::from_counts(UsageCounts {
            input: record.tokens.input,
            output: record.tokens.output,
            reasoning: record.tokens.reasoning,
            cache_write: record.tokens.cache_write,
            cache_read: record.tokens.cache_read,
            requests: record.requests,
            completions: record.completions,
            errors: record.errors,
            rate_limits: record.rate_limits,
            cost,
        });
        let agent = record.agent.or_else(|| Some("unknown".into()));
        let model = record.model.or_else(|| Some("unknown".into()));
        if record.errors > 0 {
            if let Some(error) = record
                .extra
                .get("error")
                .and_then(|error| local_error(error, timestamp))
            {
                if snapshot
                    .last_error
                    .as_ref()
                    .is_none_or(|current| current.at <= error.at)
                {
                    snapshot.last_error = Some(error);
                }
            }
        }
        snapshot.entries.push(LocalUsageEntry {
            model,
            agent,
            metrics,
        });
    }
    build_provider_usage_detail(snapshot)
}
