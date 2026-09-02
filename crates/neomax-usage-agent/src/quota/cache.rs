use std::time::Duration;

use chrono::Utc;
use neomax_core::config::Engine;
use neomax_core::usage::{ProviderUsageCache, UsageCacheStore};

pub(crate) const DEFAULT_FRESH_SECS: i64 = 60;
pub(crate) const DEFAULT_STALE_SECS: i64 = 24 * 60 * 60;
pub(crate) const CODEX_FRESH_SECS: i64 = 12;

pub(crate) fn now_epoch() -> i64 {
    Utc::now().timestamp()
}

pub(crate) fn fresh(cache: &ProviderUsageCache, now: i64, source: &str, age: Duration) -> bool {
    cache.source.as_deref() == Some(source)
        && observed_epoch(cache.observed_at)
            .is_some_and(|observed| now.saturating_sub(observed) < age.as_secs() as i64)
}

pub(crate) fn stale_ok(cache: &ProviderUsageCache, now: i64, age: Duration) -> bool {
    observed_epoch(cache.observed_at)
        .is_some_and(|observed| now.saturating_sub(observed) <= age.as_secs() as i64)
}

fn observed_epoch(value: Option<f64>) -> Option<i64> {
    value
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= i64::MAX as f64)
        .map(|value| value as i64)
}

pub(crate) fn load(
    store: &UsageCacheStore,
    engine: Engine,
    profile: &std::path::Path,
) -> Option<ProviderUsageCache> {
    store.load(engine, profile)
}
