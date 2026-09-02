use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::Engine;
use crate::accounts::{
    AccountSnapshot, FIVE_HOUR_HARD_PERCENT, QuotaSnapshotSource, engine_has_five_hour,
    rotation_advice,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaTarget {
    pub engine: Engine,
    pub profile: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaWindow {
    FiveHour,
    Weekly,
}

impl QuotaWindow {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FiveHour => "5h",
            Self::Weekly => "weekly",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaRotationAdvice {
    pub rotate: bool,
    pub reason: String,
    pub resets_at: Option<DateTime<Utc>>,
    pub limit_window: Option<QuotaWindow>,
}

pub fn quota_advice(
    quota: &dyn QuotaSnapshotSource,
    target: &QuotaTarget,
    now: DateTime<Utc>,
) -> QuotaRotationAdvice {
    let mut account = AccountSnapshot {
        engine: target.engine,
        account: String::new(),
        profile: target.profile.clone(),
        binary_available: false,
        authenticated: true,
        rotation_eligible: false,
        paused: false,
        reserved: false,
        live_workers: 0,
        five_hour_percent: None,
        weekly_percent: None,
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    };
    account.apply_quota(&quota.quota_snapshot(target.engine, &target.profile), now);
    let five = account.five_hour_at(now);
    let weekly = account.weekly_at(now);
    let advice = rotation_advice(target.engine, five, weekly);
    if !advice.rotate {
        return QuotaRotationAdvice {
            rotate: false,
            reason: advice.reason,
            resets_at: None,
            limit_window: None,
        };
    }
    let five_limit = engine_has_five_hour(target.engine) && five >= FIVE_HOUR_HARD_PERCENT;
    QuotaRotationAdvice {
        rotate: true,
        reason: advice.reason,
        resets_at: if five_limit {
            account.five_hour_reset_at
        } else {
            account.weekly_reset_at
        },
        limit_window: Some(if five_limit {
            QuotaWindow::FiveHour
        } else {
            QuotaWindow::Weekly
        }),
    }
}
