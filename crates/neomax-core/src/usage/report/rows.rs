use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Engine;

use super::details::ProviderUsageDetail;
use super::UsageMetrics;
use crate::usage::pricing::ModelPrice;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsageRow {
    pub provider: Engine,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountUsageRow {
    pub provider: Engine,
    pub account: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelUsageRow {
    pub provider: Engine,
    pub model: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateUsageRow {
    pub date: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionUsageRow {
    pub provider: Engine,
    pub session: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentUsageRow {
    pub provider: Engine,
    pub account: String,
    pub agent: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageReport {
    pub days: u32,
    pub now: i64,
    pub grand: UsageMetrics,
    pub by_provider: Vec<ProviderUsageRow>,
    pub by_account: Vec<AccountUsageRow>,
    pub by_model: Vec<ModelUsageRow>,
    pub by_date: Vec<DateUsageRow>,
    pub by_session: Vec<SessionUsageRow>,
    pub by_agent: Vec<AgentUsageRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opencode: Vec<ProviderUsageDetail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kimi: Vec<ProviderUsageDetail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grok: Vec<ProviderUsageDetail>,
    pub pricing: BTreeMap<String, ModelPrice>,
}
