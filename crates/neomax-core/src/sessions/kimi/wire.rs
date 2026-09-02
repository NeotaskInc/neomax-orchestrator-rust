use std::collections::BTreeMap;

use serde_json::Value;

use super::super::artifacts::{json_lines, Artifact};
use super::super::headers::timestamp_epoch;
use super::super::types::{FileActivity, SessionTokens};

#[derive(Debug, Default)]
pub(super) struct WireStats {
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
    pub(super) progress: bool,
}

impl WireStats {
    pub(super) fn merge(&mut self, other: &Self) {
        self.tokens.add_assign(&other.tokens);
        self.last_active = self.last_active.max(other.last_active);
        self.requests = self.requests.saturating_add(other.requests);
        self.completions = self.completions.saturating_add(other.completions);
        self.errors = self.errors.saturating_add(other.errors);
        self.rate_limits = self.rate_limits.saturating_add(other.rate_limits);
        self.tool_calls = self.tool_calls.saturating_add(other.tool_calls);
        self.tool_errors = self.tool_errors.saturating_add(other.tool_errors);
        self.progress |= other.progress;
        if other.model.is_some() {
            self.model = other.model.clone();
        }
        for (path, file) in &other.files {
            let row = self.files.entry(path.clone()).or_default();
            row.adds = row.adds.saturating_add(file.adds);
            row.dels = row.dels.saturating_add(file.dels);
            row.ops = row.ops.saturating_add(file.ops);
            row.path = path.clone();
        }
    }
}

pub(super) fn stats(artifact: &Artifact) -> WireStats {
    let mut stats = WireStats {
        last_active: artifact.modified,
        ..WireStats::default()
    };
    for event in json_lines(&artifact.text()) {
        let timestamp = event
            .get("time")
            .or_else(|| event.get("timestamp"))
            .and_then(timestamp_epoch)
            .unwrap_or(artifact.modified);
        stats.last_active = stats.last_active.max(timestamp);
        match event.get("type").and_then(Value::as_str) {
            Some("usage.record") => {
                let usage = event.get("usage").unwrap_or(&Value::Null);
                let current = SessionTokens {
                    input: integer(usage, &["inputOther", "input", "input_tokens"]),
                    output: integer(usage, &["output", "output_tokens"]),
                    cache_read: integer(usage, &["inputCacheRead", "cache_read"]),
                    cache_write: integer(usage, &["inputCacheCreation", "cache_write"]),
                    reasoning: integer(usage, &["reasoning", "reasoning_tokens"]),
                    ..SessionTokens::default()
                };
                stats.tokens.add_assign(&current);
                stats.requests = stats.requests.saturating_add(1);
                stats.completions = stats.completions.saturating_add(1);
                stats.model = event.get("model").and_then(Value::as_str).map(Into::into);
                stats.progress = true;
            }
            Some("context.append_loop_event") => {
                stats.progress = true;
                let nested = event.get("event").unwrap_or(&Value::Null);
                if nested.get("type").and_then(Value::as_str) == Some("tool.call") {
                    stats.tool_calls = stats.tool_calls.saturating_add(1);
                    if let Some(path) = nested
                        .get("args")
                        .and_then(|args| args.get("file_path").or_else(|| args.get("filePath")))
                        .and_then(Value::as_str)
                    {
                        let row = stats.files.entry(path.into()).or_default();
                        row.path = path.into();
                        row.ops = row.ops.saturating_add(1);
                    }
                }
            }
            Some("error") | Some("usage.error") => {
                stats.errors = stats.errors.saturating_add(1);
                let text = event.to_string().to_ascii_lowercase();
                if text.contains("429") || text.contains("rate limit") {
                    stats.rate_limits = stats.rate_limits.saturating_add(1);
                }
            }
            _ => {}
        }
    }
    stats
}

fn integer(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or_default()
}
