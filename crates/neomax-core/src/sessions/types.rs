use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::providers::TokenUsage;
use crate::Engine;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    #[default]
    Main,
    NativeSubagent,
    TranscriptSubagent,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionTokens {
    #[serde(default, rename = "in")]
    pub input: u64,
    #[serde(default, rename = "out")]
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default, rename = "cr")]
    pub cache_read: u64,
    #[serde(default, rename = "cw")]
    pub cache_write: u64,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub cost: f64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl SessionTokens {
    pub fn add_assign(&mut self, other: &Self) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.reasoning = self.reasoning.saturating_add(other.reasoning);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.total = self.total.saturating_add(other.total);
        self.cost += other.cost;
    }
}

impl From<TokenUsage> for SessionTokens {
    fn from(value: TokenUsage) -> Self {
        Self {
            input: value.input,
            output: value.output,
            reasoning: value.reasoning,
            cache_read: value.cache_read,
            cache_write: value.cache_write,
            total: value.total,
            cost: value.cost,
            extra: value.raw,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileActivity {
    pub path: String,
    #[serde(default)]
    pub adds: u64,
    #[serde(default)]
    pub dels: u64,
    #[serde(default)]
    pub ops: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub engine: Engine,
    #[serde(default)]
    pub account: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub kind: SessionKind,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub workflow: Option<String>,
    #[serde(default)]
    pub started: Option<i64>,
    #[serde(default)]
    pub last_active: Option<i64>,
    #[serde(default)]
    pub age_s: Option<i64>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub working: bool,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub orchestrator: bool,
    #[serde(default)]
    pub worker: bool,
    #[serde(default)]
    pub tokens: SessionTokens,
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub completions: u64,
    #[serde(default)]
    pub errors: u64,
    #[serde(default)]
    pub rate_limits: u64,
    #[serde(default)]
    pub tool_calls: u64,
    #[serde(default)]
    pub tool_errors: u64,
    #[serde(default)]
    pub files: Vec<FileActivity>,
    #[serde(default)]
    pub children: Vec<SessionRecord>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for SessionRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            engine: Engine::Claude,
            account: String::new(),
            model: None,
            cwd: None,
            project: None,
            branch: None,
            slug: None,
            label: None,
            kind: SessionKind::Main,
            parent_id: None,
            workflow: None,
            started: None,
            last_active: None,
            age_s: None,
            active: false,
            working: false,
            done: false,
            archived: false,
            orchestrator: false,
            worker: false,
            tokens: SessionTokens::default(),
            requests: 0,
            completions: 0,
            errors: 0,
            rate_limits: 0,
            tool_calls: 0,
            tool_errors: 0,
            files: Vec::new(),
            children: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

impl SessionRecord {
    pub fn with_identity(
        id: impl Into<String>,
        engine: Engine,
        account: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            engine,
            account: account.into(),
            ..Self::default()
        }
    }

    pub fn is_child(&self) -> bool {
        self.parent_id.is_some() || !matches!(self.kind, SessionKind::Main)
    }

    pub fn update_age(&mut self, now: i64) {
        self.age_s = self.last_active.map(|last| now.saturating_sub(last).max(0));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionSummary {
    #[serde(default)]
    pub sessions: u64,
    #[serde(default)]
    pub mains: u64,
    #[serde(default)]
    pub subagents: u64,
    #[serde(default)]
    pub active: u64,
    #[serde(default)]
    pub working: u64,
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub completions: u64,
    pub errors: u64,
    #[serde(default)]
    pub rate_limits: u64,
    #[serde(default)]
    pub tool_calls: u64,
    #[serde(default)]
    pub tool_errors: u64,
    #[serde(default)]
    pub files: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_tokens_add_without_overflowing() {
        let mut left = SessionTokens {
            input: u64::MAX,
            output: 2,
            ..SessionTokens::default()
        };
        left.add_assign(&SessionTokens {
            input: 4,
            output: 3,
            ..SessionTokens::default()
        });
        assert_eq!(left.input, u64::MAX);
        assert_eq!(left.output, 5);
    }

    #[test]
    fn unknown_record_fields_round_trip() {
        let value = serde_json::json!({
            "id": "session",
            "engine": "opencode",
            "future": {"preserve": true}
        });
        let record: SessionRecord = serde_json::from_value(value).unwrap();
        assert_eq!(record.extra["future"]["preserve"], true);
        let encoded = serde_json::to_value(record).unwrap();
        assert_eq!(encoded["future"]["preserve"], true);
    }
}
