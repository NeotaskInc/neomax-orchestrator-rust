use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::types::SessionTokens;

pub const SESSION_COLUMNS: &str = "id, project_id, parent_id, directory, title, agent, model, time_created, time_updated, time_archived, summary_additions, summary_deletions, summary_files, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write, cost";
pub const MESSAGE_COLUMNS: &str = "id, session_id, time_created, time_updated, data";
pub const PART_COLUMNS: &str = "id, message_id, session_id, time_created, time_updated, data";
pub const SESSION_QUERY: &str = "SELECT id, project_id, parent_id, directory, title, agent, model, time_created, time_updated, time_archived, summary_additions, summary_deletions, summary_files, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write, cost FROM session ORDER BY time_updated DESC";
pub const MESSAGE_QUERY: &str =
    "SELECT id, session_id, time_created, time_updated, data FROM message ORDER BY time_created";
pub const PART_QUERY: &str =
    "SELECT id, message_id, session_id, time_created, time_updated, data FROM part";
pub const SESSION_DISCOVERY_QUERY: &str = "SELECT id, parent_id, directory, title, agent, model, time_created, time_updated, time_archived, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write, cost FROM session ORDER BY time_updated DESC";
pub const MESSAGE_DISCOVERY_QUERY: &str = "SELECT id, session_id, time_created, data FROM message";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenCodeSessionRow {
    pub id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub directory: Option<PathBuf>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub updated: Option<i64>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub summary_additions: u64,
    #[serde(default)]
    pub summary_deletions: u64,
    #[serde(default)]
    pub summary_files: u64,
    #[serde(default)]
    pub tokens: SessionTokens,
    #[serde(default)]
    pub cost: f64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenCodeMessage {
    pub id: String,
    pub session_id: String,
    pub created: i64,
    #[serde(default)]
    pub updated: Option<i64>,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenCodePart {
    pub id: String,
    pub message_id: String,
    pub session_id: String,
    pub created: i64,
    #[serde(default)]
    pub updated: Option<i64>,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenCodeDatabase {
    pub path: PathBuf,
    #[serde(default)]
    pub sessions: Vec<OpenCodeSessionRow>,
    #[serde(default)]
    pub messages: Vec<OpenCodeMessage>,
    #[serde(default)]
    pub parts: Vec<OpenCodePart>,
}
