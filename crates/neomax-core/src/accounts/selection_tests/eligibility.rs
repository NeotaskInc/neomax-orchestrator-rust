use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    AccountSelector, FIVE_HOUR_SOFT_PERCENT, SelectionPolicy, SelectionTier, WEEKLY_HARD_PERCENT,
    select_account,
};
use super::fixtures::{account, now};

#[test]
fn excludes_the_hard_wall_and_spreads_live_work() {
    let now = now();
    let accounts = [
        account("full", 99.0, 10.0, 0),
        account("busy", 10.0, 10.0, 2),
        account("fresh", 20.0, 10.0, 0),
    ];
    let selected = select_account(
        &accounts,
        &AccountSelector::Auto,
        &BTreeSet::new(),
        &BTreeMap::new(),
        now,
        &SelectionPolicy::default(),
    )
    .unwrap();
    assert_eq!(selected.account.account, "fresh");
}

#[test]
fn explicit_selection_overrides_pause_but_not_reserved_or_missing_auth() {
    let now = now();
    let mut paused = account("2", 99.0, 99.0, 20);
    paused.paused = true;
    let selected = select_account(
        std::slice::from_ref(&paused),
        &AccountSelector::Account("2".into()),
        &BTreeSet::new(),
        &BTreeMap::new(),
        now,
        &SelectionPolicy::default(),
    )
    .unwrap();
    assert_eq!(selected.tier, SelectionTier::Explicit);
    paused.reserved = true;
    assert!(
        select_account(
            &[paused],
            &AccountSelector::Account("2".into()),
            &BTreeSet::new(),
            &BTreeMap::new(),
            now,
            &SelectionPolicy::default(),
        )
        .is_err()
    );
}

#[test]
fn soft_and_hard_walls_are_applied_at_exact_boundaries() {
    let now = now();
    let policy = SelectionPolicy::default();
    let below_soft = account("below-soft", FIVE_HOUR_SOFT_PERCENT - 0.1, 0.0, 0);
    let at_soft = account("at-soft", FIVE_HOUR_SOFT_PERCENT, 0.0, 0);
    let accounts = [below_soft, at_soft];
    let selected = select_account(
        &accounts,
        &AccountSelector::Auto,
        &BTreeSet::new(),
        &BTreeMap::new(),
        now,
        &policy,
    )
    .unwrap();
    assert_eq!(selected.account.account, "below-soft");

    let at_weekly_wall = account("weekly-wall", 0.0, WEEKLY_HARD_PERCENT, 0);
    let accounts = [at_weekly_wall];
    assert!(
        select_account(
            &accounts,
            &AccountSelector::Auto,
            &BTreeSet::new(),
            &BTreeMap::new(),
            now,
            &policy,
        )
        .is_err()
    );
}

#[test]
fn capacity_fallback_keeps_work_moving_when_every_account_is_busy() {
    let now = now();
    let policy = SelectionPolicy {
        live_concurrency_cap: 1,
        ..SelectionPolicy::default()
    };
    let busy = account("busy", 0.0, 0.0, 1);
    let accounts = [busy];
    let selected = select_account(
        &accounts,
        &AccountSelector::Auto,
        &BTreeSet::new(),
        &BTreeMap::new(),
        now,
        &policy,
    )
    .unwrap();
    assert_eq!(selected.tier, SelectionTier::CapacityFallback);
    assert_eq!(selected.account.account, "busy");
}

#[test]
fn capacity_fallback_does_not_bypass_an_active_cooldown() {
    let now = now();
    let policy = SelectionPolicy {
        live_concurrency_cap: 1,
        ..SelectionPolicy::default()
    };
    let mut cooled = account("cooled", 0.0, 0.0, 1);
    cooled.cooldown_until = Some(now + chrono::Duration::minutes(5));
    assert!(
        select_account(
            &[cooled],
            &AccountSelector::Auto,
            &BTreeSet::new(),
            &BTreeMap::new(),
            now,
            &policy,
        )
        .is_err()
    );
}

#[test]
fn automatic_and_explicit_selection_fail_closed_when_the_binary_is_missing() {
    let now = now();
    let mut unavailable = account("missing-binary", 0.0, 0.0, 0);
    unavailable.binary_available = false;
    assert!(
        select_account(
            std::slice::from_ref(&unavailable),
            &AccountSelector::Auto,
            &BTreeSet::new(),
            &BTreeMap::new(),
            now,
            &SelectionPolicy::default(),
        )
        .is_err()
    );
    let error = match select_account(
        std::slice::from_ref(&unavailable),
        &AccountSelector::Account("missing-binary".into()),
        &BTreeSet::new(),
        &BTreeMap::new(),
        now,
        &SelectionPolicy::default(),
    ) {
        Ok(_) => panic!("missing provider binary must not be selected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("executable is unavailable"));
}
