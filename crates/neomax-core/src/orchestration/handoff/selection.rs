use std::path::Path;

use chrono::{DateTime, Utc};

use crate::accounts::{
    AccountRankingPolicy, AccountSnapshot, DEFAULT_LIVE_SPREAD_WEIGHT,
    DEFAULT_WEEKLY_TIEBREAK_WEIGHT, FIVE_HOUR_SOFT_PERCENT, WEEKLY_HARD_PERCENT,
    WEEKLY_SOFT_PERCENT, compare_account_rank, engine_has_five_hour, rank_account,
};
use crate::{Engine, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetTier {
    Explicit,
    SoftHeadroom,
    HardHeadroom,
    Reserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetEligibility {
    Eligible,
    WrongEngine,
    CurrentProfile,
    MissingBinary,
    Unauthenticated,
    Paused,
    CoolingDown,
    FiveHourNearWall,
    WeeklyAtWall,
    ReservedForWorkers,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetPolicy {
    pub five_hour_skip_percent: f64,
    pub weekly_soft_percent: f64,
    pub weekly_hard_percent: f64,
    pub allow_reserved: bool,
    pub live_weight: f64,
    pub weekly_reset_weight: f64,
}

impl Default for TargetPolicy {
    fn default() -> Self {
        Self {
            five_hour_skip_percent: FIVE_HOUR_SOFT_PERCENT,
            weekly_soft_percent: WEEKLY_SOFT_PERCENT,
            weekly_hard_percent: WEEKLY_HARD_PERCENT,
            allow_reserved: false,
            live_weight: DEFAULT_LIVE_SPREAD_WEIGHT,
            weekly_reset_weight: DEFAULT_WEEKLY_TIEBREAK_WEIGHT,
        }
    }
}

impl TargetPolicy {
    fn account_ranking(&self) -> AccountRankingPolicy {
        AccountRankingPolicy {
            live_weight: self.live_weight,
            weekly_tiebreak_weight: self.weekly_reset_weight,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandoffTargetRequest<'a> {
    pub accounts: &'a [AccountSnapshot],
    pub engine: Engine,
    pub current_profile: &'a Path,
    pub selectors: &'a [String],
    pub now: DateTime<Utc>,
    pub policy: &'a TargetPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetSelection {
    pub account: AccountSnapshot,
    pub tier: TargetTier,
}

pub fn select_target(request: &HandoffTargetRequest<'_>) -> Result<TargetSelection> {
    let selectors = parse_account_selectors(request.selectors);
    if !selectors.is_empty() {
        for selector in selectors {
            if let Some(account) = request.accounts.iter().find(|account| {
                account.engine == request.engine && matches_selector(account, &selector)
            }) {
                if eligible(account, request, false) == TargetEligibility::Eligible {
                    return Ok(TargetSelection {
                        account: account.clone(),
                        tier: TargetTier::Explicit,
                    });
                }
            }
        }
        return Err(Error::Conflict(format!(
            "requested {engine} handoff account(s) are unavailable, current, paused, cooled, unauthenticated, or at the usage wall",
            engine = request.engine
        )));
    }

    for (tier, weekly_limit) in [
        (TargetTier::SoftHeadroom, request.policy.weekly_soft_percent),
        (TargetTier::HardHeadroom, request.policy.weekly_hard_percent),
    ] {
        let candidates = request
            .accounts
            .iter()
            .filter(|account| eligible(account, request, false) == TargetEligibility::Eligible)
            .filter(|account| account.weekly_at(request.now) < weekly_limit)
            .collect::<Vec<_>>();
        if let Some(account) = pick_best(&candidates, request.now, request.policy) {
            return Ok(TargetSelection {
                account: account.clone(),
                tier,
            });
        }
    }
    Err(Error::Message(format!(
        "no other logged-in {engine} account has quota headroom to hand off to",
        engine = request.engine
    )))
}

pub fn select_reserved_orchestrator(request: &HandoffTargetRequest<'_>) -> Result<TargetSelection> {
    let candidate = request
        .accounts
        .iter()
        .filter(|account| account.reserved)
        .filter(|account| eligible(account, request, true) == TargetEligibility::Eligible)
        .min_by(|left, right| compare_rank(left, right, request.now, request.policy))
        .cloned()
        .ok_or_else(|| {
            Error::Message(format!(
                "no available reserved {engine} orchestrator account",
                engine = request.engine
            ))
        })?;
    Ok(TargetSelection {
        account: candidate,
        tier: TargetTier::Reserved,
    })
}

pub fn eligibility(
    account: &AccountSnapshot,
    engine: Engine,
    current_profile: &Path,
    now: DateTime<Utc>,
    policy: &TargetPolicy,
) -> TargetEligibility {
    if account.engine != engine {
        return TargetEligibility::WrongEngine;
    }
    if same_path(&account.profile, current_profile) {
        return TargetEligibility::CurrentProfile;
    }
    if !account.binary_available {
        return TargetEligibility::MissingBinary;
    }
    if !account.authenticated {
        return TargetEligibility::Unauthenticated;
    }
    if account.paused {
        return TargetEligibility::Paused;
    }
    if account.cooldown_until.is_some_and(|until| until > now) {
        return TargetEligibility::CoolingDown;
    }
    if engine_has_five_hour(engine) && account.five_hour_at(now) >= policy.five_hour_skip_percent {
        return TargetEligibility::FiveHourNearWall;
    }
    if account.weekly_at(now) >= policy.weekly_hard_percent {
        return TargetEligibility::WeeklyAtWall;
    }
    if account.reserved && !policy.allow_reserved {
        return TargetEligibility::ReservedForWorkers;
    }
    TargetEligibility::Eligible
}

pub fn parse_account_selectors(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|value| {
            let value = value.trim().trim_end_matches(',');
            let lower = value.to_ascii_lowercase();
            if lower.is_empty()
                || matches!(
                    lower.as_str(),
                    "account" | "accounts" | "acct" | "accts" | "to" | "and"
                )
            {
                return None;
            }
            if let Some(number) = number_word(&lower) {
                return Some(number.to_string());
            }
            if let Some(number) = lower
                .strip_prefix("account")
                .or_else(|| lower.strip_prefix("acct"))
                .map(|suffix| {
                    suffix
                        .strip_prefix('-')
                        .or_else(|| suffix.strip_prefix('_'))
                        .unwrap_or(suffix)
                })
                .filter(|suffix| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()))
            {
                return Some(number.to_string());
            }
            Some(value.to_string())
        })
        .collect()
}

fn eligible(
    account: &AccountSnapshot,
    request: &HandoffTargetRequest<'_>,
    allow_reserved: bool,
) -> TargetEligibility {
    let mut policy = request.policy.clone();
    policy.allow_reserved = allow_reserved || request.policy.allow_reserved;
    eligibility(
        account,
        request.engine,
        request.current_profile,
        request.now,
        &policy,
    )
}

fn matches_selector(account: &AccountSnapshot, selector: &str) -> bool {
    let selector = selector.trim();
    account.account.eq_ignore_ascii_case(selector)
        || account
            .profile
            .to_string_lossy()
            .eq_ignore_ascii_case(selector)
        || account
            .profile
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(selector))
}

fn pick_best<'a>(
    candidates: &[&'a AccountSnapshot],
    now: DateTime<Utc>,
    policy: &TargetPolicy,
) -> Option<&'a AccountSnapshot> {
    candidates
        .iter()
        .copied()
        .min_by(|left, right| compare_rank(left, right, now, policy))
}

fn compare_rank(
    left: &AccountSnapshot,
    right: &AccountSnapshot,
    now: DateTime<Utc>,
    policy: &TargetPolicy,
) -> std::cmp::Ordering {
    compare_account_rank(
        rank_account(left, now, left.live_workers, &policy.account_ranking()),
        rank_account(right, now, right.live_workers, &policy.account_ranking()),
    )
    .then_with(|| left.account.cmp(&right.account))
}

fn same_path(left: &Path, right: &Path) -> bool {
    left == right
}

fn number_word(value: &str) -> Option<&'static str> {
    Some(match value {
        "one" => "1",
        "two" => "2",
        "three" => "3",
        "four" => "4",
        "five" => "5",
        "six" => "6",
        "seven" => "7",
        "eight" => "8",
        "nine" => "9",
        "ten" => "10",
        _ => return None,
    })
}
