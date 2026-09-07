use crate::providers::catalog::{self, CredentialEvidence, Environment, FileSystem};
use crate::usage::{ProviderUsageCache, UsageCacheStore};
use crate::{Engine, Result, StatePaths};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct QuotaEvidence {
    pub freshness: &'static str,
    pub observed_at: Option<f64>,
    pub age_seconds: Option<f64>,
    pub five_hour_percent: Option<f64>,
    pub weekly_percent: Option<f64>,
}

pub fn quota_evidence(cache: Option<&ProviderUsageCache>, now: i64) -> QuotaEvidence {
    let observed = cache
        .and_then(|cache| cache.observed_at)
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= now as f64);
    let age = observed.map(|value| now as f64 - value);
    QuotaEvidence {
        freshness: if cache.is_none() {
            "unknown"
        } else if cache.is_some_and(|cache| cache.expired || cache.stale)
            || age.is_some_and(|age| age > 300.0)
        {
            "stale"
        } else if age.is_some() {
            "fresh"
        } else {
            "unknown"
        },
        observed_at: observed,
        age_seconds: age,
        five_hour_percent: cache
            .and_then(|cache| cache.five_hour.used_percent)
            .filter(|value| value.is_finite() && (0.0..=100.0).contains(value)),
        weekly_percent: cache
            .and_then(|cache| cache.seven_day.used_percent)
            .filter(|value| value.is_finite() && (0.0..=100.0).contains(value)),
    }
}

#[derive(Debug, Serialize)]
pub struct AccountDiagnostic {
    pub engine: Engine,
    pub account: String,
    pub profile: PathBuf,
    pub email: Option<String>,
    pub authenticated: bool,
    pub credential: CredentialEvidence,
    pub quota: QuotaEvidence,
}

pub fn inspect_accounts(
    paths: &StatePaths,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
    now: i64,
) -> Result<Vec<AccountDiagnostic>> {
    let usage = UsageCacheStore::new(&paths.usage);
    let mut result = Vec::new();
    for engine in Engine::ALL {
        for profile in catalog::discover_profile_snapshots(engine, environment, filesystem)? {
            result.push(AccountDiagnostic {
                engine,
                email: catalog::profile_email_with_environment(
                    engine,
                    &profile.path,
                    &paths.home,
                    environment,
                    filesystem,
                ),
                credential: catalog::credential_evidence(
                    &profile,
                    &paths.home,
                    environment,
                    filesystem,
                    now,
                ),
                quota: quota_evidence(usage.load_read_only(engine, &profile.path).as_ref(), now),
                authenticated: profile.eligibility.authenticated,
                account: profile.account,
                profile: profile.path,
            });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn freshness_never_invents_a_timestamp_or_zero_usage() {
        let absent = quota_evidence(None, 1000);
        assert_eq!(absent.freshness, "unknown");
        assert!(absent.weekly_percent.is_none());
        let mut cache = ProviderUsageCache::default();
        assert_eq!(quota_evidence(Some(&cache), 1000).freshness, "unknown");
        cache.observed_at = Some(900.0);
        assert_eq!(quota_evidence(Some(&cache), 1000).freshness, "fresh");
        assert_eq!(quota_evidence(Some(&cache), 1300).freshness, "stale");
        cache.observed_at = Some(5000.0);
        assert_eq!(quota_evidence(Some(&cache), 1000).freshness, "unknown");
        cache.stale = true;
        assert_eq!(quota_evidence(Some(&cache), 1000).freshness, "stale");
    }
}
