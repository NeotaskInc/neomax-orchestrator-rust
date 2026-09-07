use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{Engine, Result};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelQuotaWindow {
    #[serde(default, deserialize_with = "optional_number")]
    pub used_percent: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub resets_at: Option<f64>,
}

fn optional_number<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<f64>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        serde_json::Value::Number(value) => value.as_f64(),
        serde_json::Value::String(value) => value.parse().ok(),
        _ => None,
    }))
}

/// Provider-independent quota data supplied to account policy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuotaSnapshot {
    pub available: bool,
    pub five_hour_percent: Option<f64>,
    pub weekly_percent: Option<f64>,
    pub model_weekly: BTreeMap<String, crate::accounts::ModelQuotaWindow>,
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    pub weekly_reset_at: Option<DateTime<Utc>>,
    pub expired: bool,
}

/// Supplies the latest locally available quota observation for an account.
pub trait QuotaSnapshotSource: Send + Sync {
    fn quota_snapshot(&self, engine: Engine, profile: &Path) -> QuotaSnapshot;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveWorkSnapshot {
    pub counts: BTreeMap<(Engine, PathBuf), u32>,
}

impl LiveWorkSnapshot {
    pub fn count(&self, engine: Engine, profile: &Path) -> u32 {
        self.counts
            .get(&(engine, profile.to_path_buf()))
            .copied()
            .unwrap_or(0)
    }
}

/// Supplies live worker counts without exposing run persistence to account policy.
pub trait LiveWorkSource: Send + Sync {
    fn live_work(&self) -> Result<LiveWorkSnapshot>;
}
