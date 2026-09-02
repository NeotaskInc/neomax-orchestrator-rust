use chrono::Duration;

use super::super::{
    AccountRankingPolicy, AccountSelector, SelectionPolicy, compare_account_rank, rank_account,
    select_account,
};
use super::fixtures::{account, now};
use std::collections::{BTreeMap, BTreeSet};

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
