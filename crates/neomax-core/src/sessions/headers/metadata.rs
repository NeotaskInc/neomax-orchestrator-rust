use std::collections::BTreeMap;

use serde_json::Value;

use crate::sessions::artifacts::{flatten_extra, json_lines};

use super::identity::timestamp_epoch;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeaderMetadata {
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub slug: Option<String>,
    pub label: Option<String>,
    pub session_id: Option<String>,
    pub started: Option<i64>,
    pub extra: BTreeMap<String, Value>,
}

pub fn claude_head_meta(head: &str) -> HeaderMetadata {
    let mut meta = HeaderMetadata::default();
    for event in json_lines(head) {
        if meta.extra.is_empty() {
            if let Some(object) = event.as_object() {
                meta.extra = flatten_extra(
                    object,
                    &[
                        "type",
                        "cwd",
                        "gitBranch",
                        "slug",
                        "sessionId",
                        "timestamp",
                        "message",
                        "model",
                    ],
                );
            }
        }
        if meta.cwd.is_none() {
            meta.cwd = string_field(&event, "cwd");
            meta.branch = string_field(&event, "gitBranch");
            meta.slug = string_field(&event, "slug");
        }
        if meta.session_id.is_none() {
            meta.session_id = string_field(&event, "sessionId");
        }
        if meta.started.is_none() {
            meta.started = event.get("timestamp").and_then(timestamp_epoch);
        }
        if meta.label.is_none() && event.get("type").and_then(Value::as_str) == Some("user") {
            let content = event
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(content_text);
            if let Some(content) = content.filter(|value| !is_metadata_text(value)) {
                meta.label = Some(bound_label(&content, 110));
            }
        }
        if meta.cwd.is_some() && meta.label.is_some() && meta.started.is_some() {
            break;
        }
    }
    meta
}

pub fn codex_head_meta(head: &str) -> HeaderMetadata {
    let mut meta = HeaderMetadata::default();
    for event in json_lines(head) {
        if meta.extra.is_empty() {
            if let Some(object) = event.as_object() {
                meta.extra = flatten_extra(
                    object,
                    &["type", "cwd", "session_id", "timestamp", "payload", "model"],
                );
            }
        }
        let payload = event.get("payload").unwrap_or(&event);
        if meta.cwd.is_none() {
            meta.cwd = string_field(payload, "cwd").or_else(|| string_field(&event, "cwd"));
        }
        if meta.branch.is_none() {
            meta.branch =
                string_field(payload, "branch").or_else(|| string_field(payload, "git_branch"));
        }
        if meta.session_id.is_none() {
            meta.session_id =
                string_field(payload, "session_id").or_else(|| string_field(&event, "session_id"));
        }
        if meta.started.is_none() {
            meta.started = payload
                .get("timestamp")
                .and_then(timestamp_epoch)
                .or_else(|| event.get("timestamp").and_then(timestamp_epoch));
        }
        let event_type = payload
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| event.get("type").and_then(Value::as_str));
        if meta.label.is_none() && event_type == Some("user_message") {
            if let Some(message) = payload.get("message").and_then(Value::as_str) {
                meta.label = Some(bound_label(message, 110));
            }
        }
        if meta.cwd.is_some() && meta.label.is_some() && meta.started.is_some() {
            break;
        }
    }
    meta
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn content_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    value.as_array()?.iter().find_map(|block| {
        block
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn is_metadata_text(value: &str) -> bool {
    value.starts_with("Caveat:")
        || value.starts_with("<command-")
        || value.starts_with("<local-command")
        || value.starts_with("<system-reminder")
        || value.starts_with("<task-notification")
}

fn bound_label(value: &str, limit: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
