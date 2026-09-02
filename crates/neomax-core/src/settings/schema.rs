use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::constants::{DEFAULT_FLEET_LIVE_CAP, DEFAULT_QUEUE_TTL_SECONDS};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsFile {
    pub concurrency: ConcurrencySettings,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConcurrencySettings {
    pub max_subagents: u32,
    pub max_tasks: u32,
    pub max_sessions_per_account: u32,
    pub lanes_per_account: u32,
    /// Runtime-only fleet cap, kept separate from the persisted subagent budget.
    #[serde(skip, default = "default_fleet_live_cap")]
    pub fleet_live_cap: Option<u32>,
    pub queue_ttl_seconds: f64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

fn default_fleet_live_cap() -> Option<u32> {
    Some(DEFAULT_FLEET_LIVE_CAP)
}

impl Default for ConcurrencySettings {
    fn default() -> Self {
        Self {
            max_subagents: 50,
            max_tasks: 0,
            max_sessions_per_account: 10,
            lanes_per_account: 6,
            fleet_live_cap: Some(DEFAULT_FLEET_LIVE_CAP),
            queue_ttl_seconds: DEFAULT_QUEUE_TTL_SECONDS,
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EffectiveSettings {
    pub concurrency: ConcurrencySettings,
    pub config_path: PathBuf,
    pub max_subagents_source: String,
}
