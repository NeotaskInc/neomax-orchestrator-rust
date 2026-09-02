use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueClaim {
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub ts: i64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl IssueClaim {
    pub fn new(session: Option<String>, pid: Option<u32>, ts: i64) -> Self {
        Self {
            session,
            pid,
            ts,
            extra: BTreeMap::new(),
        }
    }
}
