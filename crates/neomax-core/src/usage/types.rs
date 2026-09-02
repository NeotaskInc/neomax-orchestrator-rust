use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::providers::TokenUsage;
use crate::Engine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    pub engine: Engine,
    pub account: String,
    pub model: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub run_id: Option<String>,
    pub tokens: TokenUsage,
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub errors: u64,
    #[serde(default)]
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedgerKind {
    Add,
    Total,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerRecord {
    pub ts: i64,
    #[serde(rename = "provider")]
    pub engine: Engine,
    pub account: String,
    pub model: String,
    pub id: String,
    pub kind: LedgerKind,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default, rename = "in")]
    pub input: u64,
    #[serde(default, rename = "out")]
    pub output: u64,
    #[serde(default)]
    pub reasoning: u64,
    #[serde(default, rename = "cw")]
    pub cache_write: u64,
    #[serde(default, rename = "cr")]
    pub cache_read: u64,
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub requests: Option<u64>,
    #[serde(default)]
    pub completions: Option<u64>,
    #[serde(default)]
    pub errors: u64,
    #[serde(default)]
    pub rate_limits: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl LedgerRecord {
    pub fn total_tokens(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.reasoning)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }
}
