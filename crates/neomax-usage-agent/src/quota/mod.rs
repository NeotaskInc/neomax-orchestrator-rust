mod cache;
mod claude;
mod codex;
mod http;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
pub use neomax_core::accounts::QuotaSupport;
use neomax_core::accounts::quota_support;
use neomax_core::config::Engine;
use neomax_core::usage::{ProviderUsageCache, UsageCacheStore};
use serde::Serialize;

use crate::collector::{ProfileCatalog, account_name};
use crate::config::AgentPaths;

pub use http::JsonHttp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuotaProviderReport {
    pub engine: Engine,
    pub account: String,
    pub capability: QuotaSupport,
    pub attempted: bool,
    pub refreshed: bool,
    pub stale: bool,
    pub source: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct QuotaReport {
    pub providers: Vec<QuotaProviderReport>,
    pub errors: u64,
}

pub trait QuotaRefresher: Send + Sync {
    fn refresh(&self, force: bool) -> Result<QuotaReport>;

    fn refresh_after_rate_limit(&self) -> Result<QuotaReport> {
        self.refresh(true)
    }
}

pub struct LocalQuotaRefresher {
    paths: AgentPaths,
    http: Arc<dyn JsonHttp>,
    use_keychain: bool,
}

impl LocalQuotaRefresher {
    pub fn new(paths: AgentPaths) -> Self {
        Self {
            paths,
            http: Arc::new(http::ReqwestHttp),
            use_keychain: true,
        }
    }

    pub fn with_http(paths: AgentPaths, http: Arc<dyn JsonHttp>) -> Self {
        Self {
            paths,
            http,
            use_keychain: false,
        }
    }

    pub fn with_http_and_keychain(paths: AgentPaths, http: Arc<dyn JsonHttp>) -> Self {
        Self {
            paths,
            http,
            use_keychain: true,
        }
    }

    pub fn refresh_profile(
        &self,
        engine: Engine,
        profile: &Path,
        force: bool,
    ) -> QuotaProviderReport {
        let account = account_name(profile);
        let capability = quota_support(engine);
        let result = match capability {
            QuotaSupport::Reactive => Ok(None),
            QuotaSupport::Numeric => match engine {
                Engine::Claude => claude::refresh(
                    &self.paths,
                    profile,
                    self.http.as_ref(),
                    force,
                    self.use_keychain,
                ),
                Engine::Codex => Ok(codex::refresh(&self.paths, profile, force)),
                Engine::Opencode | Engine::Kimi | Engine::Grok => {
                    unreachable!("reactive providers are handled before numeric quota refresh")
                }
            },
        };
        match result {
            Ok(Some(cache)) => self.persist(engine, profile, account, cache, force),
            Ok(None) => QuotaProviderReport {
                engine,
                account,
                capability,
                attempted: matches!(capability, QuotaSupport::Numeric),
                refreshed: false,
                stale: false,
                source: None,
                error: None,
            },
            Err(_) => QuotaProviderReport {
                engine,
                account,
                capability,
                attempted: true,
                refreshed: false,
                stale: false,
                source: None,
                error: Some("quota refresh failed".into()),
            },
        }
    }

    fn persist(
        &self,
        engine: Engine,
        profile: &Path,
        account: String,
        cache: ProviderUsageCache,
        force: bool,
    ) -> QuotaProviderReport {
        let stale = cache.stale;
        let source = cache.source.clone();
        let expired = cache.expired;
        let should_save = !stale && (force || cache.observed_at.is_some());
        let error = if should_save {
            let store = UsageCacheStore::new(&self.paths.state.usage);
            store
                .save(engine, profile, &cache)
                .err()
                .map(|_| "quota cache could not be written".into())
        } else {
            None
        };
        QuotaProviderReport {
            engine,
            account,
            capability: quota_support(engine),
            attempted: true,
            refreshed: !stale && error.is_none() && !expired,
            stale,
            source,
            error,
        }
    }
}

impl QuotaRefresher for LocalQuotaRefresher {
    fn refresh(&self, force: bool) -> Result<QuotaReport> {
        self.paths.state.ensure_runtime_dirs()?;
        let profiles = ProfileCatalog::discover(&self.paths);
        let mut report = QuotaReport::default();
        for profile in profiles.for_engine(Engine::Claude) {
            report
                .providers
                .push(self.refresh_profile(Engine::Claude, profile, force));
        }
        for profile in profiles.for_engine(Engine::Codex) {
            report
                .providers
                .push(self.refresh_profile(Engine::Codex, profile, force));
        }
        report.errors = report
            .providers
            .iter()
            .filter(|provider| provider.error.is_some())
            .count() as u64;
        Ok(report)
    }
}

impl neomax_core::accounts::QuotaSnapshotSource for LocalQuotaRefresher {
    fn quota_snapshot(
        &self,
        engine: Engine,
        profile: &Path,
    ) -> neomax_core::accounts::QuotaSnapshot {
        let store = UsageCacheStore::new(&self.paths.state.usage);
        let Some(cache) = cache::load(&store, engine, profile) else {
            return neomax_core::accounts::QuotaSnapshot::default();
        };
        let numeric = matches!(quota_support(engine), QuotaSupport::Numeric);
        let reactive_weekly = matches!(quota_support(engine), QuotaSupport::Reactive)
            && cache.has_trustworthy_weekly();
        if !numeric && !reactive_weekly {
            return neomax_core::accounts::QuotaSnapshot::default();
        }
        neomax_core::accounts::QuotaSnapshot {
            available: true,
            five_hour_percent: numeric.then_some(cache.five_hour.used_percent).flatten(),
            weekly_percent: cache.seven_day.used_percent,
            five_hour_reset_at: numeric
                .then_some(cache.five_hour.resets_at.and_then(epoch_datetime))
                .flatten(),
            weekly_reset_at: cache.seven_day.resets_at.and_then(epoch_datetime),
            expired: cache.expired,
        }
    }
}

fn epoch_datetime(value: f64) -> Option<DateTime<Utc>> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    DateTime::from_timestamp_millis((value * 1000.0) as i64)
}

#[cfg(test)]
mod tests;
