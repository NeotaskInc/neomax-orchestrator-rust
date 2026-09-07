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
    pub model_weekly: std::collections::BTreeMap<String, crate::accounts::ModelQuotaWindow>,
    #[serde(default)]
    pub cooldown_until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub five_hour_reset_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub weekly_reset_at: Option<DateTime<Utc>>,
}

impl AccountSnapshot {
    pub fn limiting_model_family(&self, model: &str, now: DateTime<Utc>) -> Option<&'static str> {
        if self.engine == Engine::Claude && !self.at_hard_wall(now)
            && self.for_model(model, now).at_hard_wall(now)
        {
            crate::accounts::claude_model_family(model)
        } else {
            None
        }
    }

    pub fn for_model(&self, model: &str, now: DateTime<Utc>) -> Self {
        let mut scoped = self.clone();
        if self.engine != Engine::Claude {
            return scoped;
        }
        let Some(family) = crate::accounts::claude_model_family(model) else {
            return scoped;
        };
        if let Some(window) = self.model_weekly.get(family) {
            let reset = window.resets_at.and_then(|value| {
                value.is_finite().then(|| DateTime::from_timestamp(value as i64, 0)).flatten()
            });
            let percent = window_percent(window.used_percent, reset, now);
            if percent > self.weekly_at(now) {
                scoped.weekly_percent = Some(percent);
                scoped.weekly_reset_at = reset;
            }
        }
        scoped
    }

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
        self.model_weekly = quota.model_weekly.clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fable_allowance_intersects_shared_quota_without_disabling_other_models() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut account: AccountSnapshot = serde_json::from_value(json!({
            "engine":"claude", "account":"1", "profile":"/profiles/one", "authenticated":true,
            "five_hour_percent":20, "weekly_percent":40,
            "model_weekly":{"fable":{"used_percent":100,"resets_at":1_800_003_600}}
        })).unwrap();
        for model in ["claude-fable-5", "claude-fable-5-1[1m]", "fable"] {
            assert!(account.for_model(model, now).at_hard_wall(now));
            assert_eq!(account.limiting_model_family(model, now), Some("fable"));
        }
        for model in ["claude-opus-5[1m]", "claude-sonnet-5", "future-model"] {
            assert!(!account.for_model(model, now).at_hard_wall(now));
        }
        assert_eq!(account.weekly_percent, Some(40.0));
        let after_reset = now + chrono::Duration::hours(2);
        assert!(!account.for_model("fable", after_reset).at_hard_wall(after_reset));
        account.weekly_percent = Some(99.0);
        assert!(account.for_model("claude-opus-5", now).at_hard_wall(now));
        assert!(account.for_model("fable", now).at_hard_wall(now));
        assert_eq!(account.limiting_model_family("fable", now), None);
    }
}
