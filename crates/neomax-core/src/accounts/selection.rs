use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::{EffectiveSettings, Error, Result};

use super::snapshot::AccountSnapshot;
use super::windows::{
    DEFAULT_LIVE_SPREAD_WEIGHT, DEFAULT_WEEKLY_TIEBREAK_WEIGHT, FIVE_HOUR_HARD_PERCENT,
    FIVE_HOUR_SOFT_PERCENT, WEEKLY_HARD_PERCENT, WEEKLY_SOFT_PERCENT, engine_has_five_hour,
    weekly_deadline_tier,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountSelector {
    Auto,
    Account(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionTier {
    Explicit,
    SoftHeadroom,
    HardHeadroom,
    CapacityFallback,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectionPolicy {
    pub live_concurrency_cap: u32,
    pub live_spread_weight: f64,
    pub five_skip_percent: f64,
    pub five_hard_percent: f64,
    pub weekly_skip_percent: f64,
    pub weekly_hard_percent: f64,
    pub weekly_tiebreak_weight: f64,
    pub weekly_bucket_seconds: f64,
    pub weekly_horizon_seconds: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccountRankingPolicy {
    pub live_weight: f64,
    pub weekly_tiebreak_weight: f64,
}

impl Default for AccountRankingPolicy {
    fn default() -> Self {
        Self {
            live_weight: DEFAULT_LIVE_SPREAD_WEIGHT,
            weekly_tiebreak_weight: DEFAULT_WEEKLY_TIEBREAK_WEIGHT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccountRank {
    pub at_five_hour_hard_wall: bool,
    pub score: f64,
    pub weekly_percent: f64,
}

impl Default for SelectionPolicy {
    fn default() -> Self {
        Self {
            live_concurrency_cap: 10,
            live_spread_weight: DEFAULT_LIVE_SPREAD_WEIGHT,
            five_skip_percent: FIVE_HOUR_SOFT_PERCENT,
            five_hard_percent: FIVE_HOUR_HARD_PERCENT,
            weekly_skip_percent: WEEKLY_SOFT_PERCENT,
            weekly_hard_percent: WEEKLY_HARD_PERCENT,
            weekly_tiebreak_weight: DEFAULT_WEEKLY_TIEBREAK_WEIGHT,
            weekly_bucket_seconds: super::windows::WEEKLY_BUCKET_SECONDS,
            weekly_horizon_seconds: super::windows::WEEKLY_HORIZON_SECONDS,
        }
    }
}

impl SelectionPolicy {
    pub fn from_settings(settings: &EffectiveSettings) -> Self {
        Self {
            live_concurrency_cap: settings.concurrency.max_sessions_per_account,
            ..Self::default()
        }
    }

    pub fn ranking(&self) -> AccountRankingPolicy {
        AccountRankingPolicy {
            live_weight: self.live_spread_weight,
            weekly_tiebreak_weight: self.weekly_tiebreak_weight,
        }
    }
}

pub struct SelectionDecision<'a> {
    pub account: &'a AccountSnapshot,
    pub tier: SelectionTier,
    pub score: f64,
}

pub fn rank_account(
    account: &AccountSnapshot,
    now: DateTime<Utc>,
    contention: u32,
    policy: &AccountRankingPolicy,
) -> AccountRank {
    let five_hour = account.five_hour_at(now);
    AccountRank {
        at_five_hour_hard_wall: engine_has_five_hour(account.engine)
            && five_hour >= FIVE_HOUR_HARD_PERCENT,
        score: five_hour
            + f64::from(contention) * policy.live_weight
            + weekly_deadline_tier(account.weekly_reset_at, now) * policy.weekly_tiebreak_weight,
        weekly_percent: account.weekly_at(now),
    }
}

pub fn compare_account_rank(left: AccountRank, right: AccountRank) -> std::cmp::Ordering {
    left.at_five_hour_hard_wall
        .cmp(&right.at_five_hour_hard_wall)
        .then_with(|| left.score.total_cmp(&right.score))
        .then_with(|| left.weekly_percent.total_cmp(&right.weekly_percent))
}

pub fn select_account<'a>(
    accounts: &'a [AccountSnapshot],
    selector: &AccountSelector,
    excluded: &BTreeSet<PathBuf>,
    bias: &BTreeMap<PathBuf, f64>,
    now: DateTime<Utc>,
    policy: &SelectionPolicy,
) -> Result<SelectionDecision<'a>> {
    if let AccountSelector::Account(account) = selector {
        return select_explicit(accounts, account);
    }
    let soft = accounts
        .iter()
        .filter(|account| {
            normal_eligible(account, excluded, now, policy, policy.weekly_skip_percent)
        })
        .collect::<Vec<_>>();
    if let Some(decision) = pick_best(&soft, bias, now, policy, SelectionTier::SoftHeadroom) {
        return Ok(decision);
    }
    let hard = accounts
        .iter()
        .filter(|account| {
            normal_eligible(account, excluded, now, policy, policy.weekly_hard_percent)
        })
        .collect::<Vec<_>>();
    if let Some(decision) = pick_best(&hard, bias, now, policy, SelectionTier::HardHeadroom) {
        return Ok(decision);
    }
    let fallback = accounts
        .iter()
        .filter(|account| {
            !excluded.contains(&account.profile)
                && account.binary_available
                && account.authenticated
                && !account.paused
                && !account.reserved
                && !account.at_hard_wall(now)
                && account.cooldown_until.is_none_or(|until| until <= now)
                && account.weekly_at(now) < policy.weekly_hard_percent
                && account.five_hour_at(now) < policy.five_hard_percent
        })
        .collect::<Vec<_>>();
    pick_best(
        &fallback,
        bias,
        now,
        policy,
        SelectionTier::CapacityFallback,
    )
    .ok_or_else(|| {
        Error::Message(format!(
            "no authenticated account has quota headroom below the {:.0} percent wall",
            FIVE_HOUR_HARD_PERCENT.min(WEEKLY_HARD_PERCENT)
        ))
    })
}

fn select_explicit<'a>(
    accounts: &'a [AccountSnapshot],
    requested: &str,
) -> Result<SelectionDecision<'a>> {
    let account = accounts
        .iter()
        .find(|account| account.account.eq_ignore_ascii_case(requested))
        .ok_or_else(|| Error::NotFound(format!("account {requested}")))?;
    if !account.binary_available {
        return Err(Error::Message(format!(
            "account {requested} provider executable is unavailable"
        )));
    }
    if account.reserved {
        return Err(Error::Conflict(format!(
            "account {requested} is reserved for orchestration"
        )));
    }
    if !account.authenticated {
        return Err(Error::Message(format!(
            "account {requested} is not authenticated"
        )));
    }
    Ok(SelectionDecision {
        account,
        tier: SelectionTier::Explicit,
        score: 0.0,
    })
}

fn normal_eligible(
    account: &AccountSnapshot,
    excluded: &BTreeSet<PathBuf>,
    now: DateTime<Utc>,
    policy: &SelectionPolicy,
    weekly_cap: f64,
) -> bool {
    !excluded.contains(&account.profile)
        && account.binary_available
        && account.authenticated
        && !account.paused
        && !account.reserved
        && !account.at_hard_wall(now)
        && account.live_workers < policy.live_concurrency_cap
        && account.cooldown_until.is_none_or(|until| until <= now)
        && account.five_hour_at(now) < policy.five_skip_percent
        && account.weekly_at(now) < weekly_cap
}

fn pick_best<'a>(
    candidates: &[&'a AccountSnapshot],
    bias: &BTreeMap<PathBuf, f64>,
    now: DateTime<Utc>,
    policy: &SelectionPolicy,
    tier: SelectionTier,
) -> Option<SelectionDecision<'a>> {
    candidates
        .iter()
        .map(|account| {
            let rank = rank_account(account, now, account.live_workers, &policy.ranking());
            let score = rank.score
                + rank.weekly_percent
                + bias.get(&account.profile).copied().unwrap_or(0.0);
            SelectionDecision {
                account,
                tier,
                score,
            }
        })
        .min_by(|left, right| {
            let left_rank = rank_account(
                left.account,
                now,
                left.account.live_workers,
                &policy.ranking(),
            );
            let right_rank = rank_account(
                right.account,
                now,
                right.account.live_workers,
                &policy.ranking(),
            );
            left_rank
                .at_five_hour_hard_wall
                .cmp(&right_rank.at_five_hour_hard_wall)
                .then_with(|| left.score.total_cmp(&right.score))
                .then_with(|| {
                    left_rank
                        .weekly_percent
                        .total_cmp(&right_rank.weekly_percent)
                })
                .then_with(|| left.account.account.cmp(&right.account.account))
        })
}

#[cfg(test)]
#[path = "selection_tests/mod.rs"]
mod tests;
