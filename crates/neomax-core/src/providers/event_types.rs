use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
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
    pub total: u64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub raw: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildActivity {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub status: String,
    #[serde(default)]
    pub last_tool: Option<String>,
    #[serde(default)]
    pub tokens: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParsedEvents {
    #[serde(default)]
    pub result_text: Option<String>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub api_error_status: Option<String>,
    #[serde(default)]
    pub rate_limited: bool,
    #[serde(default)]
    pub resets_at: Option<f64>,
    #[serde(default)]
    pub limit_window: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub children: Vec<ChildActivity>,
    #[serde(default)]
    pub usage: TokenUsage,
}
