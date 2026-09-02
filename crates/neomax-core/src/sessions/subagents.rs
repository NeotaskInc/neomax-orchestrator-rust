use std::collections::BTreeMap;

use serde_json::Value;

use super::artifacts::flatten_extra;
use super::types::{FileActivity, SessionKind, SessionRecord, SessionTokens};

pub fn normalize_child(mut child: SessionRecord, parent: &SessionRecord) -> SessionRecord {
    child.kind = SessionKind::NativeSubagent;
    child.parent_id = Some(parent.id.clone());
    if child.cwd.is_none() {
        child.cwd = parent.cwd.clone();
    }
    if child.project.is_none() {
        child.project = parent.project.clone();
    }
    if child.branch.is_none() {
        child.branch = parent.branch.clone();
    }
    child.account = parent.account.clone();
    child.engine = parent.engine;
    child
}

pub fn child_from_value(
    value: &Value,
    parent: &SessionRecord,
    id: impl Into<String>,
    label: Option<String>,
) -> SessionRecord {
    let id = id.into();
    let mut child = SessionRecord::with_identity(id, parent.engine, parent.account.clone());
    child.kind = SessionKind::NativeSubagent;
    child.parent_id = Some(parent.id.clone());
    child.cwd = value
        .get("cwd")
        .or_else(|| value.get("directory"))
        .and_then(Value::as_str)
        .map(Into::into)
        .or_else(|| parent.cwd.clone());
    child.model = value
        .get("model")
        .and_then(Value::as_str)
        .map(Into::into)
        .or_else(|| parent.model.clone());
    child.label = label.or_else(|| value.get("title").and_then(Value::as_str).map(Into::into));
    child.project = parent.project.clone();
    child.branch = parent.branch.clone();
    child.started = integer(value, &["started", "created", "created_at"]);
    child.last_active = integer(value, &["last_active", "updated", "updated_at"]);
    child.active = value
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    child.working = child.active;
    child.done = value
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| matches!(status, "done" | "completed" | "finished"));
    child.tokens = tokens(value.get("tokens"));
    child.requests = integer(value, &["requests"]).unwrap_or_default().max(0) as u64;
    child.completions = integer(value, &["completions"]).unwrap_or_default().max(0) as u64;
    child.errors = integer(value, &["errors"]).unwrap_or_default().max(0) as u64;
    child.rate_limits = integer(value, &["rate_limits"]).unwrap_or_default().max(0) as u64;
    child.tool_calls = integer(value, &["tool_calls"]).unwrap_or_default().max(0) as u64;
    child.tool_errors = integer(value, &["tool_errors"]).unwrap_or_default().max(0) as u64;
    child.files = files(value.get("files"));
    child.extra = value.as_object().map_or_else(BTreeMap::new, |object| {
        flatten_extra(
            object,
            &[
                "id",
                "session_id",
                "agent_id",
                "cwd",
                "directory",
                "model",
                "title",
                "label",
                "started",
                "created",
                "created_at",
                "last_active",
                "updated",
                "updated_at",
                "active",
                "status",
                "done",
                "tokens",
                "requests",
                "completions",
                "errors",
                "rate_limits",
                "tool_calls",
                "tool_call_count",
                "tool_errors",
                "files",
            ],
        )
    });
    child
}

pub fn child_records_from_events(
    events: impl IntoIterator<Item = Value>,
    parent: &SessionRecord,
) -> Vec<SessionRecord> {
    let mut children = BTreeMap::<String, SessionRecord>::new();
    for event in events {
        let Some(object) = event.as_object() else {
            continue;
        };
        let candidate = object
            .get("subagent")
            .or_else(|| object.get("child"))
            .or_else(|| object.get("agent"));
        let Some(value) = candidate else {
            continue;
        };
        let value = if value.is_object() { value } else { &event };
        let id = value
            .get("id")
            .or_else(|| value.get("session_id"))
            .or_else(|| value.get("agent_id"))
            .and_then(Value::as_str);
        let Some(id) = id else {
            continue;
        };
        let row = children.entry(id.to_string()).or_insert_with(|| {
            child_from_value(
                value,
                parent,
                id,
                value.get("label").and_then(Value::as_str).map(Into::into),
            )
        });
        if let Some(status) = value.get("status").and_then(Value::as_str) {
            row.done = matches!(status, "done" | "completed" | "finished");
            row.active = matches!(status, "running" | "active" | "working");
            row.working = row.active;
        }
        if let Some(last) = integer(value, &["last_active", "timestamp"]) {
            row.last_active = Some(row.last_active.unwrap_or_default().max(last));
        }
        row.tool_calls = row
            .tool_calls
            .max(integer(value, &["tool_calls", "tool_call_count"]).unwrap_or_default() as u64);
        row.errors = row
            .errors
            .max(integer(value, &["errors", "error_count"]).unwrap_or_default() as u64);
    }
    children.into_values().collect()
}

pub fn merge_child_tokens(parent: &mut SessionRecord, child: &SessionRecord) {
    parent.tokens.add_assign(&child.tokens);
    parent.requests = parent.requests.saturating_add(child.requests);
    parent.completions = parent.completions.saturating_add(child.completions);
    parent.errors = parent.errors.saturating_add(child.errors);
    parent.rate_limits = parent.rate_limits.saturating_add(child.rate_limits);
    parent.tool_calls = parent.tool_calls.saturating_add(child.tool_calls);
    parent.tool_errors = parent.tool_errors.saturating_add(child.tool_errors);
}

fn tokens(value: Option<&Value>) -> SessionTokens {
    let Some(value) = value else {
        return SessionTokens::default();
    };
    SessionTokens {
        input: integer(value, &["in", "input", "input_tokens"]).unwrap_or_default() as u64,
        output: integer(value, &["out", "output", "output_tokens"]).unwrap_or_default() as u64,
        reasoning: integer(value, &["reasoning", "reasoning_tokens"]).unwrap_or_default() as u64,
        cache_read: integer(value, &["cr", "cache_read", "cache_read_input_tokens"])
            .unwrap_or_default() as u64,
        cache_write: integer(value, &["cw", "cache_write", "cache_creation_input_tokens"])
            .unwrap_or_default() as u64,
        total: integer(value, &["total", "total_tokens"]).unwrap_or_default() as u64,
        cost: value
            .get("cost")
            .and_then(Value::as_f64)
            .unwrap_or_default(),
        extra: BTreeMap::new(),
    }
}

fn files(value: Option<&Value>) -> Vec<FileActivity> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|file| {
            if let Some(path) = file.as_str() {
                return Some(FileActivity {
                    path: path.into(),
                    ..FileActivity::default()
                });
            }
            Some(FileActivity {
                path: file
                    .get("path")
                    .or_else(|| file.get("file_path"))
                    .and_then(Value::as_str)?
                    .into(),
                adds: integer(file, &["adds", "additions"]).unwrap_or_default() as u64,
                dels: integer(file, &["dels", "deletions"]).unwrap_or_default() as u64,
                ops: integer(file, &["ops", "operations"]).unwrap_or_default() as u64,
                extra: BTreeMap::new(),
            })
        })
        .collect()
}

fn integer(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        value.get(*key).and_then(|item| {
            item.as_i64()
                .or_else(|| item.as_u64().and_then(|number| i64::try_from(number).ok()))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;

    #[test]
    fn normalizes_child_identity_and_inherits_project_context() {
        let mut parent = SessionRecord::with_identity("parent", Engine::Grok, "acct");
        parent.cwd = Some("/repo".into());
        parent.project = Some("project".into());
        let child = child_from_value(
            &serde_json::json!({"id":"child","status":"running","tokens":{"out":4},"future":true}),
            &parent,
            "child",
            Some("inspect".into()),
        );
        assert_eq!(child.parent_id.as_deref(), Some("parent"));
        assert_eq!(child.project.as_deref(), Some("project"));
        assert_eq!(child.tokens.output, 4);
        assert_eq!(child.extra["future"], true);
    }
}
