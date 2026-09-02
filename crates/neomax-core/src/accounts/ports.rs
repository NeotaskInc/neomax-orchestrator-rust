use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::{Engine, Result};

/// Provider-independent quota data supplied to account policy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuotaSnapshot {
    pub available: bool,
    pub five_hour_percent: Option<f64>,
    pub weekly_percent: Option<f64>,
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
