use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::providers::ParsedEvents;

use super::response::{find_snapshot, parse_window};
use super::window::{raw_reset, select_reset};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexQuotaRefreshReason {
    UsageLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexQuotaRefreshRequest {
    pub method: String,
    pub timeout_ms: u64,
    pub reason: CodexQuotaRefreshReason,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CodexQuotaWindow {
    pub used_percent: Option<f64>,
    pub window_minutes: Option<u64>,
    pub resets_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexQuotaRefreshResult {
    pub observed_at: f64,
    pub primary: Option<CodexQuotaWindow>,
    pub secondary: Option<CodexQuotaWindow>,
    pub rate_limit_reached_type: Option<String>,
    pub resets_at: Option<f64>,
    pub limit_window: Option<String>,
    pub rate_limits: Value,
}

impl CodexQuotaRefreshResult {
    pub fn from_value(value: Value, observed_at: f64) -> Option<Self> {
        let snapshot = find_snapshot(&value)?;
        let primary = parse_window(snapshot.get("primary"));
        let secondary = parse_window(snapshot.get("secondary"));
        let rate_limit_reached_type = snapshot
            .get("rateLimitReachedType")
            .or_else(|| snapshot.get("rate_limit_reached_type"))
            .and_then(Value::as_str)
            .map(str::to_string);
        if primary.is_none() && secondary.is_none() && rate_limit_reached_type.is_none() {
            return None;
        }
        let (resets_at, limit_window) =
            select_reset(primary.as_ref(), secondary.as_ref(), observed_at);
        Some(Self {
            observed_at,
            primary,
            secondary,
            rate_limit_reached_type,
            resets_at,
            limit_window,
            rate_limits: value,
        })
    }

    pub fn blocks_new_work(&self) -> bool {
        self.rate_limit_reached_type
            .as_deref()
            .is_some_and(|value| !value.is_empty())
            || self
                .primary
                .as_ref()
                .into_iter()
                .chain(self.secondary.as_ref())
                .any(|window| window.used_percent.is_some_and(|value| value >= 99.0))
    }

    pub fn apply_to(&self, parsed: &mut ParsedEvents) {
        let fallback = raw_reset(self);
        let fallback_reset = fallback.as_ref().map(|(reset, _)| *reset);
        let fallback_window = fallback.and_then(|(_, window)| window);
        parsed.resets_at = self.resets_at.or(fallback_reset).or(parsed.resets_at);
        if let Some(window) = self.limit_window.clone() {
            parsed.limit_window = Some(window);
        } else if parsed.limit_window.is_none() {
            parsed.limit_window = fallback_window;
        }
    }
}
