use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Engine;

use super::ports::QuotaSnapshot;
use super::windows::{at_hard_wall, engine_has_five_hour, window_percent};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub engine: Engine,
    pub account: String,
    pub profile: PathBuf,
    /// Whether the provider executable was present when this inventory was built.
    /// Routing must fail closed when discovery cannot prove the binary exists.
    #[serde(default)]
    pub binary_available: bool,
    pub authenticated: bool,
    /// Whether this profile's credentials may be copied or swapped in place.
    /// API-key-only profiles remain pool-eligible but must use a handoff.
    #[serde(default)]
    pub rotation_eligible: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub reserved: bool,
    #[serde(default)]
    pub live_workers: u32,
    #[serde(default)]
    pub five_hour_percent: Option<f64>,
    #[serde(default)]
    pub weekly_percent: Option<f64>,
    #[serde(default)]
    pub cooldown_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub weekly_reset_at: Option<DateTime<Utc>>,
}

impl AccountSnapshot {
    pub fn apply_quota(&mut self, quota: &QuotaSnapshot, now: DateTime<Utc>) {
        if !quota.available {
            return;
        }
        if quota.expired {
            if engine_has_five_hour(self.engine) {
                self.five_hour_percent = Some(100.0);
            }
            self.weekly_percent = Some(0.0);
            return;
        }
        self.five_hour_reset_at = quota.five_hour_reset_at;
        self.weekly_reset_at = quota.weekly_reset_at;
        self.five_hour_percent = if engine_has_five_hour(self.engine)
            && !quota.five_hour_reset_at.is_some_and(|reset| reset <= now)
        {
            quota.five_hour_percent
        } else {
            Some(0.0)
        };
        self.weekly_percent = if quota.weekly_reset_at.is_some_and(|reset| reset <= now) {
            Some(0.0)
        } else {
            quota.weekly_percent
        };
    }

    pub fn five_hour_at(&self, now: DateTime<Utc>) -> f64 {
        if !engine_has_five_hour(self.engine) {
            0.0
        } else {
            window_percent(self.five_hour_percent, self.five_hour_reset_at, now)
        }
    }

    pub fn weekly_at(&self, now: DateTime<Utc>) -> f64 {
        window_percent(self.weekly_percent, self.weekly_reset_at, now)
    }

    pub fn at_hard_wall(&self, now: DateTime<Utc>) -> bool {
        at_hard_wall(self.engine, self.five_hour_at(now), self.weekly_at(now))
    }

    pub fn measured_load(&self, now: DateTime<Utc>, live_weight: f64) -> f64 {
        self.five_hour_at(now) + self.weekly_at(now) + f64::from(self.live_workers) * live_weight
    }
}
