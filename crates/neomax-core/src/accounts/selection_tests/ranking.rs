use chrono::Duration;

use super::super::{
    AccountRankingPolicy, AccountSelector, SelectionPolicy, compare_account_rank, rank_account,
    select_account,
};
use super::fixtures::{account, now};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn reset_aware_opt_in_breaks_same_day_ties_without_relaxing_eligibility() {
    let now = now();
    let mut later = account("1", 10.0, 10.0, 0);
    later.weekly_reset_at = Some(now + Duration::hours(20));
    let mut sooner = account("2", 10.0, 10.0, 0);
    sooner.weekly_reset_at = Some(now + Duration::hours(1));
    let mut accounts = [later, sooner];
    let excluded = BTreeSet::new();
    let bias = BTreeMap::new();
    let mut policy = SelectionPolicy::default();
    let pick = |accounts: &[super::super::AccountSnapshot], policy: &SelectionPolicy| {
        select_account(
            accounts,
            &AccountSelector::Auto,
            &excluded,
            &bias,
            now,
            policy,
        )
        .unwrap()
        .account
        .account
        .clone()
    };
    assert_eq!(pick(&accounts, &policy), "1");
    policy.reset_aware = true;
    assert_eq!(pick(&accounts, &policy), "2");
    accounts[1].paused = true;
    assert_eq!(pick(&accounts, &policy), "1");
    accounts[1].paused = false;
    accounts[1].weekly_percent = Some(99.0);
    assert_eq!(pick(&accounts, &policy), "1");
    accounts[1].weekly_percent = Some(10.0);
    accounts[1].cooldown_until = Some(now + Duration::hours(1));
    assert_eq!(pick(&accounts, &policy), "1");
}

#[test]
fn weekly_deadline_pressure_breaks_otherwise_equal_load() {
    let now = now();
    let mut later = account("later", 10.0, 10.0, 0);
    later.weekly_reset_at = Some(now + Duration::days(5));
    let mut sooner = account("sooner", 10.0, 10.0, 0);
    sooner.weekly_reset_at = Some(now + Duration::hours(5));
    let accounts = [later, sooner];
    let selected = select_account(
        &accounts,
        &AccountSelector::Auto,
        &BTreeSet::new(),
        &BTreeMap::new(),
        now,
        &SelectionPolicy::default(),
    )
    .unwrap();
    assert_eq!(selected.account.account, "sooner");
}

#[test]
fn account_ranking_prefers_the_soonest_weekly_reset_after_equal_five_hour_load() {
    let now = now();
    let mut later = account("later", 10.0, 80.0, 0);
    later.weekly_reset_at = Some(now + Duration::days(5));
    let mut sooner = account("sooner", 10.0, 80.0, 0);
    sooner.weekly_reset_at = Some(now + Duration::hours(5));
    let ranking = AccountRankingPolicy::default();
    assert_eq!(
        compare_account_rank(
            rank_account(&sooner, now, 0, &ranking),
            rank_account(&later, now, 0, &ranking),
        ),
        std::cmp::Ordering::Less
    );
}
