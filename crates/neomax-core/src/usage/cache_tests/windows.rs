use chrono::{Duration, Utc};

use super::super::{ProviderUsageCache, QuotaWindow, UsageCacheStore};
use super::fixtures::{account, cache};
use crate::Engine;

#[test]
fn reads_compatible_window_cache_and_ignores_phantom_codex_five_hour() {
    let temp = tempfile::tempdir().unwrap();
    let store = UsageCacheStore::new(temp.path());
    let profile = temp.path().join("profiles/.codex");
    let now = Utc::now();
    store
        .save(
            Engine::Codex,
            &profile,
            &ProviderUsageCache {
                five_hour: QuotaWindow {
                    used_percent: Some(99.0),
                    resets_at: Some((now + Duration::hours(1)).timestamp() as f64),
                },
                seven_day: QuotaWindow {
                    used_percent: Some(45.0),
                    resets_at: Some((now + Duration::days(2)).timestamp() as f64),
                },
                ..ProviderUsageCache::default()
            },
        )
        .unwrap();
    let mut account = account(Engine::Codex, profile);
    store.hydrate(&mut account, now);
    assert_eq!(account.five_hour_percent, Some(0.0));
    assert_eq!(account.weekly_percent, Some(45.0));
}

#[test]
fn reactive_provider_cache_without_provenance_stays_unknown() {
    let temp = tempfile::tempdir().unwrap();
    let store = UsageCacheStore::new(temp.path());
    let profile = temp.path().join("profiles/.opencode");
    store
        .save(Engine::Opencode, &profile, &cache(97.0))
        .unwrap();
    let mut account = account(Engine::Opencode, profile);
    store.hydrate(&mut account, Utc::now());
    assert!(account.five_hour_percent.is_none());
    assert!(account.weekly_percent.is_none());
}

#[test]
fn reactive_provider_trusted_weekly_cache_triggers_hard_wall_without_five_hour() {
    let temp = tempfile::tempdir().unwrap();
    let store = UsageCacheStore::new(temp.path());
    let now = Utc::now();
    for engine in [Engine::Opencode, Engine::Kimi, Engine::Grok] {
        let profile = temp.path().join(format!("profiles/.{}", engine.as_str()));
        store
            .save(
                engine,
                &profile,
                &ProviderUsageCache {
                    seven_day: QuotaWindow {
                        used_percent: Some(99.0),
                        resets_at: Some((now + Duration::days(3)).timestamp() as f64),
                    },
                    source: Some(format!("{engine}-usage-event")),
                    observed_at: Some(now.timestamp() as f64),
                    ..ProviderUsageCache::default()
                },
            )
            .unwrap();
        let mut account = account(engine, profile);
        store.hydrate(&mut account, now);
        assert_eq!(account.five_hour_at(now), 0.0, "{engine} has no 5h window");
        assert_eq!(account.weekly_percent, Some(99.0), "{engine} weekly cache");
        assert!(account.at_hard_wall(now), "{engine} weekly hard wall");
    }
}

#[test]
fn expired_window_resets_to_zero() {
    let temp = tempfile::tempdir().unwrap();
    let store = UsageCacheStore::new(temp.path());
    let profile = temp.path().join("profiles/.claude");
    let now = Utc::now();
    store
        .save(
            Engine::Claude,
            &profile,
            &ProviderUsageCache {
                five_hour: QuotaWindow {
                    used_percent: Some(98.0),
                    resets_at: Some((now - Duration::seconds(1)).timestamp() as f64),
                },
                ..ProviderUsageCache::default()
            },
        )
        .unwrap();
    let mut account = account(Engine::Claude, profile);
    store.hydrate(&mut account, now);
    assert_eq!(account.five_hour_percent, Some(0.0));
}

#[cfg(windows)]
#[test]
fn partial_root_profiles_cannot_read_or_write_usage_cache_entries() {
    let temp = tempfile::tempdir().unwrap();
    let store = UsageCacheStore::new(temp.path());

    for raw in [r"\rooted", r"C:drive-relative"] {
        let profile = std::path::Path::new(raw);
        assert!(store.load(Engine::Claude, profile).is_none());
        assert!(store.save(Engine::Claude, profile, &cache(12.0)).is_err());
        assert_ne!(
            store.path(Engine::Claude, profile),
            store.path(Engine::Claude, std::path::Path::new(r"C:\rooted"))
        );
    }
}
