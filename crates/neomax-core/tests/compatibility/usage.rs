use std::fs;

use chrono::{DateTime, Utc};
use neomax_core::Engine;
use neomax_core::accounts::AccountSnapshot;
use neomax_core::usage::{
    LedgerKind, ProviderUsageCache, UsageCacheStore, UsageLedger, UsageReport,
};

use super::support::{
    assert_fixture_is_sanitized, assert_json_roundtrip, fixture_as, fixture_json, fixture_text,
    platform_fixture_path,
};

#[test]
fn usage_cache_accepts_numeric_strings_preserves_unknown_fields_and_hydrates_windows() {
    assert_fixture_is_sanitized("usage/cache_claude.json");
    let expected = fixture_json("usage/cache_claude.json");
    let cache: ProviderUsageCache = serde_json::from_value(expected).unwrap();
    assert_eq!(cache.five_hour.used_percent, Some(91.5));
    assert_eq!(cache.five_hour.resets_at, Some(1_787_493_600_000.0));
    assert_eq!(cache.extra["future_cache_field"]["preserve"], true);

    let temp = tempfile::tempdir().unwrap();
    let store = UsageCacheStore::new(temp.path());
    let profile = platform_fixture_path("/profiles/claude-1");
    store.save(Engine::Claude, &profile, &cache).unwrap();
    let mut account = AccountSnapshot {
        engine: Engine::Claude,
        account: "account-1".into(),
        profile,
        binary_available: true,
        authenticated: true,
        rotation_eligible: true,
        paused: false,
        reserved: false,
        live_workers: 0,
        five_hour_percent: None,
        weekly_percent: None,
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    };
    let now = DateTime::<Utc>::from_timestamp(1_787_488_123, 0).unwrap();
    store.hydrate(&mut account, now);
    assert_eq!(account.five_hour_percent, Some(91.5));
    assert_eq!(account.weekly_percent, Some(47.25));
    assert_eq!(
        account.five_hour_reset_at.unwrap().timestamp(),
        1_787_493_600
    );
}

#[test]
fn codex_cache_does_not_invent_a_five_hour_window() {
    let cache: ProviderUsageCache = fixture_as("usage/cache_codex.json");
    let temp = tempfile::tempdir().unwrap();
    let store = UsageCacheStore::new(temp.path());
    let profile = platform_fixture_path("/profiles/codex-2");
    store.save(Engine::Codex, &profile, &cache).unwrap();
    let mut account = AccountSnapshot {
        engine: Engine::Codex,
        account: "account-2".into(),
        profile,
        binary_available: true,
        authenticated: true,
        rotation_eligible: true,
        paused: false,
        reserved: false,
        live_workers: 0,
        five_hour_percent: None,
        weekly_percent: None,
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    };
    store.hydrate(
        &mut account,
        DateTime::<Utc>::from_timestamp(1_787_488_123, 0).unwrap(),
    );
    assert_eq!(account.five_hour_percent, Some(0.0));
    assert_eq!(account.weekly_percent, Some(81.0));
}

#[test]
fn usage_ledger_deduplicates_add_and_total_records_and_skips_bad_lines() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("fixture.jsonl"),
        fixture_text("usage/ledger.jsonl"),
    )
    .unwrap();
    let ledger = UsageLedger::new(temp.path());
    let rows = ledger.read_deduplicated(0, 1_787_488_123).unwrap();
    assert_eq!(rows.len(), 2);
    let add = rows.iter().find(|row| row.id == "completion-1").unwrap();
    assert_eq!(add.kind, LedgerKind::Add);
    assert_eq!(add.output, 220);
    assert_eq!(add.extra["future_ledger_field"], "preserve");
    let total = rows.iter().find(|row| row.id == "session-1").unwrap();
    assert_eq!(total.kind, LedgerKind::Total);
    assert_eq!(total.total_tokens(), 7500);
}

#[test]
fn usage_report_fixture_reserializes_exactly() {
    let expected = fixture_json("usage/report.json");
    let report: UsageReport = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(report.days, 7);
    assert_eq!(report.grand.output, 1120);
    assert_eq!(report.by_provider.len(), 2);
    assert_eq!(report.pricing["claude-fable-5[1m]"].cache_write, 12.5);
    assert_json_roundtrip(&report, &expected);
}
