use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::super::{
    HandoffTargetRequest, TargetPolicy, TargetTier, select_reserved_orchestrator, select_target,
};
use crate::Engine;
use crate::accounts::AccountSnapshot;

fn account(engine: Engine, name: &str, five: Option<f64>, weekly: Option<f64>) -> AccountSnapshot {
    AccountSnapshot {
        engine,
        account: name.into(),
        profile: PathBuf::from(format!("/profiles/{engine}-{name}")),
        binary_available: true,
        authenticated: true,
        rotation_eligible: true,
        paused: false,
        reserved: false,
        live_workers: 0,
        five_hour_percent: five,
        weekly_percent: weekly,
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    }
}

fn request<'a>(
    accounts: &'a [AccountSnapshot],
    engine: Engine,
    current: &'a Path,
    selectors: &'a [String],
    now: DateTime<Utc>,
    policy: &'a TargetPolicy,
) -> HandoffTargetRequest<'a> {
    HandoffTargetRequest {
        accounts,
        engine,
        current_profile: current,
        selectors,
        now,
        policy,
    }
}

#[test]
fn auto_selection_covers_all_five_providers() {
    let now = Utc::now();
    for engine in Engine::ALL {
        let accounts = [
            account(engine, "current", Some(10.0), Some(10.0)),
            account(engine, "fresh", Some(5.0), Some(5.0)),
        ];
        let policy = TargetPolicy::default();
        let selected = select_target(&request(
            &accounts,
            engine,
            Path::new("/profiles/current-current"),
            &[],
            now,
            &policy,
        ))
        .unwrap();
        assert_eq!(selected.account.account, "fresh");
    }
}

#[test]
fn excludes_pause_cooldown_auth_and_usage_walls() {
    let now = Utc::now();
    let mut paused = account(Engine::Claude, "paused", Some(1.0), Some(1.0));
    paused.paused = true;
    let mut cooled = account(Engine::Claude, "cooled", Some(1.0), Some(1.0));
    cooled.cooldown_until = Some(now + chrono::Duration::hours(1));
    let mut unauthenticated = account(Engine::Claude, "unauth", Some(1.0), Some(1.0));
    unauthenticated.authenticated = false;
    let near_wall = account(Engine::Claude, "near", Some(92.0), Some(1.0));
    let weekly_wall = account(Engine::Claude, "weekly", Some(1.0), Some(99.0));
    let fresh = account(Engine::Claude, "fresh", Some(2.0), Some(2.0));
    let accounts = [
        paused,
        cooled,
        unauthenticated,
        near_wall,
        weekly_wall,
        fresh,
    ];
    let policy = TargetPolicy::default();
    let selected = select_target(&request(
        &accounts,
        Engine::Claude,
        Path::new("/profiles/current-current"),
        &[],
        now,
        &policy,
    ))
    .unwrap();
    assert_eq!(selected.account.account, "fresh");
}

#[test]
fn ranks_eligible_claude_targets_by_five_hour_headroom_before_weekly_load() {
    let now = Utc::now();
    let lower_five = account(Engine::Claude, "lower-five", Some(1.0), Some(98.0));
    let lower_weekly = account(Engine::Claude, "lower-weekly", Some(2.0), Some(1.0));
    let accounts = [lower_five, lower_weekly];
    let policy = TargetPolicy::default();
    let selected = select_target(&request(
        &accounts,
        Engine::Claude,
        Path::new("/profiles/claude-current"),
        &[],
        now,
        &policy,
    ))
    .unwrap();
    assert_eq!(selected.account.account, "lower-five");
}

#[test]
fn explicit_selectors_normalize_account_words_and_reject_unusable_targets() {
    let now = Utc::now();
    let mut blocked = account(Engine::Codex, "2", None, Some(99.0));
    blocked.paused = true;
    let fresh = account(Engine::Codex, "3", None, Some(2.0));
    let accounts = [blocked, fresh];
    let policy = TargetPolicy::default();
    let selectors = vec!["account".into(), "two".into()];
    assert!(
        select_target(&request(
            &accounts,
            Engine::Codex,
            Path::new("/profiles/codex-current"),
            &selectors,
            now,
            &policy,
        ))
        .is_err()
    );

    let selectors = vec!["acct3".into()];
    let selected = select_target(&request(
        &accounts,
        Engine::Codex,
        Path::new("/profiles/codex-current"),
        &selectors,
        now,
        &policy,
    ))
    .unwrap();
    assert_eq!(selected.tier, TargetTier::Explicit);
    assert_eq!(selected.account.account, "3");
}

#[test]
fn explicit_account_selector_stays_pinned_to_the_requested_provider() {
    let now = Utc::now();
    let claude = account(Engine::Claude, "2", Some(1.0), Some(1.0));
    let kimi = account(Engine::Kimi, "2", None, Some(1.0));
    let accounts = [claude, kimi];
    let policy = TargetPolicy::default();
    let selectors = vec!["2".into()];
    let selected = select_target(&request(
        &accounts,
        Engine::Kimi,
        Path::new("/profiles/kimi-current"),
        &selectors,
        now,
        &policy,
    ))
    .unwrap();
    assert_eq!(selected.account.engine, Engine::Kimi);
    assert_eq!(selected.account.account, "2");
}

#[test]
fn reserved_selection_is_explicit_and_still_obeys_quota_safety() {
    let now = Utc::now();
    let mut reserved = account(Engine::Kimi, "orch", None, Some(2.0));
    reserved.reserved = true;
    let ordinary = account(Engine::Kimi, "1", None, Some(1.0));
    let accounts = [ordinary, reserved];
    let policy = TargetPolicy::default();
    let ordinary_target = select_target(&request(
        &accounts,
        Engine::Kimi,
        Path::new("/profiles/kimi-current"),
        &[],
        now,
        &policy,
    ))
    .unwrap();
    assert_eq!(ordinary_target.account.account, "1");
    let selected = select_reserved_orchestrator(&request(
        &accounts,
        Engine::Kimi,
        Path::new("/profiles/kimi-current"),
        &[],
        now,
        &policy,
    ))
    .unwrap();
    assert_eq!(selected.tier, TargetTier::Reserved);
    assert_eq!(selected.account.account, "orch");
}
