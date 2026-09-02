use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use neomax_core::accounts::{AccountControlStore, AccountSnapshot, RotationClaimStore};
use neomax_core::runs::HistorySummary;
use neomax_core::{Engine, Result};

use super::support::{
    assert_fixture_is_sanitized, fixture_as, fixture_json, fixture_text_with_platform_paths,
    platform_fixture_path,
};

#[test]
fn account_snapshot_preserves_provider_quota_and_timestamp_shapes() {
    assert_fixture_is_sanitized("accounts/account_snapshot.json");
    let snapshot: AccountSnapshot = fixture_as("accounts/account_snapshot.json");
    assert_eq!(snapshot.engine, Engine::Claude);
    assert_eq!(snapshot.account, "account-1");
    assert_eq!(snapshot.profile, PathBuf::from("/profiles/claude-1"));
    assert_eq!(snapshot.five_hour_percent, Some(91.5));
    assert_eq!(snapshot.weekly_percent, Some(47.25));
    assert_eq!(
        snapshot.cooldown_until,
        Some(
            DateTime::parse_from_rfc3339("2026-08-23T12:30:00Z")
                .unwrap()
                .with_timezone(&Utc)
        )
    );
    let now = DateTime::parse_from_rfc3339("2026-08-23T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    assert!(!snapshot.at_hard_wall(now));
}

#[test]
fn account_controls_read_fixture_forms_and_fail_closed_on_malformed_state() {
    let temp = tempfile::tempdir().unwrap();
    let cooldowns = temp.path().join("cooldown.json");
    let paused = temp.path().join("paused.json");
    fs::write(
        &cooldowns,
        fixture_text_with_platform_paths("accounts/cooldown.json"),
    )
    .unwrap();
    fs::write(
        &paused,
        fixture_text_with_platform_paths("accounts/paused.json"),
    )
    .unwrap();
    let store = AccountControlStore::new(&cooldowns, &paused);

    assert_eq!(store.cooldowns().len(), 2);
    assert!(
        store
            .is_paused(platform_fixture_path("/profiles/codex-2").as_path())
            .unwrap()
    );
    assert!(
        !store
            .is_paused(platform_fixture_path("/profiles/claude-1").as_path())
            .unwrap()
    );

    let malformed_cooldown = temp.path().join("malformed-cooldown.json");
    let malformed_paused = temp.path().join("malformed-paused.json");
    fs::write(&malformed_cooldown, "{").unwrap();
    fs::write(&malformed_paused, "{").unwrap();
    let malformed = AccountControlStore::new(malformed_cooldown, malformed_paused);
    assert!(malformed.cooldowns().is_empty());
    assert!(malformed.paused().is_empty());

    let missing = AccountControlStore::new(
        temp.path().join("missing-cooldown.json"),
        temp.path().join("missing-paused.json"),
    );
    assert!(missing.cooldowns().is_empty());
    assert!(missing.paused().is_empty());
}

#[test]
fn rotation_claim_fixture_is_parseable_and_account_selection_is_deterministic() {
    let temp = tempfile::tempdir().unwrap();
    let claims_path = temp.path().join("rotation-claims.json");
    fs::write(
        &claims_path,
        fixture_text_with_platform_paths("accounts/rotation_claims.json"),
    )
    .unwrap();
    let store = RotationClaimStore::new(&claims_path, temp.path().join("rotation.lock"));
    assert_eq!(store.claims().len(), 2);

    let snapshots = [
        AccountSnapshot {
            engine: Engine::Claude,
            account: "account-1".into(),
            profile: platform_fixture_path("/profiles/claude-1"),
            binary_available: true,
            authenticated: true,
            rotation_eligible: true,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: Some(10.0),
            weekly_percent: Some(30.0),
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        },
        AccountSnapshot {
            engine: Engine::Claude,
            account: "account-2".into(),
            profile: platform_fixture_path("/profiles/claude-2"),
            binary_available: true,
            authenticated: true,
            rotation_eligible: true,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: Some(20.0),
            weekly_percent: Some(30.0),
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        },
    ];
    let selected = store
        .pick_and_claim(&snapshots, Utc::now())
        .unwrap()
        .unwrap();
    assert_eq!(selected.account, "account-1");
    let rank = store.rank(&snapshots[0], &BTreeMap::new(), Utc::now(), false);
    assert!(!rank.over_five_hour_ceiling);
    assert!(rank.spread_load >= snapshots[0].five_hour_percent.unwrap());
}

#[test]
fn legacy_account_number_fixture_is_retained_as_a_contract_probe() {
    let value = fixture_json("accounts/legacy_account_number_string.json");
    assert_eq!(value["account_number"].as_str(), Some("2"));
    let legacy = serde_json::json!({
        "id": "run-compat-001",
        "engine": "codex",
        "account": "codex-2",
        "account_number": value["account_number"],
        "status": "done",
        "ultra": false,
        "opus": false,
        "children": 0,
        "attempt": 1,
        "started": 1,
        "ended": 2
    });
    let summary: HistorySummary = serde_json::from_value(legacy).unwrap();
    assert_eq!(summary.account_number, Some(2));
    assert_eq!(serde_json::to_value(summary).unwrap()["account_number"], 2);
}

#[test]
fn account_control_mutations_keep_fixture_state_atomic() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let store = AccountControlStore::new(
        temp.path().join("cooldown.json"),
        temp.path().join("paused.json"),
    );
    let profile = platform_fixture_path("/profiles/claude-1");
    store.set_paused(&profile, true)?;
    let until = store.set_cooldown(&profile, Some(1_787_490_000.9), 1_787_488_123.0, 300.0)?;
    assert_eq!(until, 1_787_490_000.0);
    assert!(store.is_paused(&profile)?);
    assert_eq!(
        store.cooldown_until(&profile, 1_787_488_124.0)?,
        Some(until)
    );
    store.clear_cooldown(&profile)?;
    store.set_paused(&profile, false)?;
    assert_eq!(store.cooldown_until(&profile, 1_787_488_124.0)?, None);
    assert!(!store.is_paused(&profile)?);
    Ok(())
}
