use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueEvent {
    #[serde(default)]
    pub ts: i64,
    #[serde(default)]
    pub event: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
