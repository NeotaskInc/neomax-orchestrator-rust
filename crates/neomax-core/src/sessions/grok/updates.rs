use std::collections::BTreeMap;

use serde_json::Value;

use super::super::artifacts::{json_lines, Artifact};
use super::super::headers::timestamp_epoch;
use super::super::types::{FileActivity, SessionTokens};
use super::usage;

#[derive(Debug, Default)]
pub(super) struct UpdateStats {
    pub(super) tokens: SessionTokens,
    pub(super) model: Option<String>,
    pub(super) last_active: i64,
    pub(super) requests: u64,
    pub(super) completions: u64,
    pub(super) errors: u64,
    pub(super) rate_limits: u64,
    pub(super) tool_calls: u64,
    pub(super) tool_errors: u64,
    pub(super) files: BTreeMap<String, FileActivity>,
    pub(super) agents: BTreeMap<String, AgentState>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct AgentState {
    pub(super) id: String,
    pub(super) label: Option<String>,
    pub(super) model: Option<String>,
    pub(super) status: String,
    pub(super) last_active: i64,
    pub(super) tool_calls: u64,
    pub(super) errors: u64,
}

pub(super) fn parse(artifact: &Artifact) -> UpdateStats {
    let mut stats = UpdateStats {
        last_active: artifact.modified,
        ..UpdateStats::default()
    };
    for envelope in json_lines(&artifact.text()) {
        let timestamp = envelope
            .get("timestamp")
            .and_then(timestamp_epoch)
            .unwrap_or(artifact.modified);
        stats.last_active = stats.last_active.max(timestamp);
        let update = envelope
            .get("params")
            .and_then(|params| params.get("update"))
            .unwrap_or(&envelope);
        let tag = update
            .get("sessionUpdate")
            .or_else(|| update.get("type"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        match tag {
            "turn_completed" => {
                if let Some(event) =
                    usage::parse_usage_line(&envelope.to_string(), "", None, artifact.modified)
                {
                    stats.tokens.add_assign(&event.tokens);
                    stats.requests = stats.requests.saturating_add(event.requests);
                    stats.completions = stats.completions.saturating_add(event.completions);
                    stats.errors = stats.errors.saturating_add(event.errors);
                    stats.rate_limits = stats.rate_limits.saturating_add(event.rate_limits);
                    if event.model.is_some() {
                        stats.model = event.model;
                    }
                }
            }
            "tool_call" => {
                stats.tool_calls = stats.tool_calls.saturating_add(1);
                collect_files(update, &mut stats.files);
            }
            "tool_call_update" => {
                if update.get("status").and_then(Value::as_str) == Some("failed") {
                    stats.tool_errors = stats.tool_errors.saturating_add(1);
                }
                collect_files(update, &mut stats.files);
            }
            "subagent_spawned" => {
                let Some(id) = update
                    .get("subagent_id")
                    .or_else(|| update.get("child_session_id"))
                    .and_then(Value::as_str)
                else {
                    continue;
                };
                stats.agents.insert(
                    id.into(),
                    AgentState {
                        id: id.into(),
                        label: update
                            .get("description")
                            .or_else(|| update.get("subagent_type"))
                            .and_then(Value::as_str)
                            .map(Into::into),
                        model: update.get("model").and_then(Value::as_str).map(Into::into),
                        status: "running".into(),
                        last_active: timestamp,
                        ..AgentState::default()
                    },
                );
            }
            "subagent_progress" | "subagent_finished" => {
                let Some(id) = update.get("subagent_id").and_then(Value::as_str) else {
                    continue;
                };
                let row = stats.agents.entry(id.into()).or_insert_with(|| AgentState {
                    id: id.into(),
                    ..AgentState::default()
                });
                row.status = if tag == "subagent_progress" {
                    "running".into()
                } else {
                    update
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("completed")
                        .into()
                };
                row.last_active = timestamp;
                row.tool_calls = integer(update, &["tool_calls", "tool_call_count"]);
                row.errors = integer(update, &["error_count", "errors"]);
            }
            _ => {}
        }
    }
    stats
}

fn collect_files(value: &Value, files: &mut BTreeMap<String, FileActivity>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(key.as_str(), "path" | "file_path" | "filePath") {
                    if let Some(path) = value.as_str() {
                        let row = files.entry(path.into()).or_default();
                        row.path = path.into();
                        row.ops = row.ops.saturating_add(1);
                    }
                } else {
                    collect_files(value, files);
                }
            }
        }
        Value::Array(values) => values.iter().for_each(|value| collect_files(value, files)),
        _ => {}
    }
}

fn integer(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or_default()
}
