use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Engine;

pub const FIVE_HOUR_SOFT_PERCENT: f64 = 92.0;
pub const FIVE_HOUR_HARD_PERCENT: f64 = 99.0;
pub const WEEKLY_SOFT_PERCENT: f64 = 99.0;
pub const WEEKLY_HARD_PERCENT: f64 = 99.0;
pub const LIVE_ROTATION_FIVE_PERCENT: f64 = FIVE_HOUR_HARD_PERCENT;
pub const LIVE_ROTATION_WEEKLY_PERCENT: f64 = WEEKLY_HARD_PERCENT;

pub const WEEKLY_BUCKET_SECONDS: f64 = 24.0 * 60.0 * 60.0;
pub const WEEKLY_HORIZON_SECONDS: f64 = 8.0 * 24.0 * 60.0 * 60.0;
pub const DEFAULT_LIVE_SPREAD_WEIGHT: f64 = 6.0;
pub const DEFAULT_WEEKLY_TIEBREAK_WEIGHT: f64 = 1.5;

// Compatibility names remain exported while all policy code uses the canonical names above.
pub const FIVE_SKIP_PERCENT: f64 = FIVE_HOUR_SOFT_PERCENT;
pub const FIVE_HARD_PERCENT: f64 = FIVE_HOUR_HARD_PERCENT;
pub const WEEKLY_SKIP_PERCENT: f64 = WEEKLY_SOFT_PERCENT;
pub const ROTATE_FIVE_PERCENT: f64 = LIVE_ROTATION_FIVE_PERCENT;
pub const ROTATE_WEEKLY_PERCENT: f64 = LIVE_ROTATION_WEEKLY_PERCENT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaSupport {
    Numeric,
    Reactive,
}

pub const fn quota_support(engine: Engine) -> QuotaSupport {
    match engine {
        Engine::Claude | Engine::Codex => QuotaSupport::Numeric,
        Engine::Opencode | Engine::Kimi | Engine::Grok => QuotaSupport::Reactive,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RotationAdvice {
    pub rotate: bool,
    pub reason: String,
}

pub const fn engine_has_five_hour(engine: Engine) -> bool {
    matches!(engine, Engine::Claude)
}

pub fn window_percent(
    percent: Option<f64>,
    reset_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> f64 {
    if reset_at.is_some_and(|reset| reset <= now) {
        0.0
    } else {
        percent.unwrap_or(0.0)
    }
}

pub fn weekly_deadline_tier(reset_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> f64 {
    let seconds = reset_at.map_or(WEEKLY_HORIZON_SECONDS, |reset| {
        if reset <= now {
            WEEKLY_HORIZON_SECONDS
        } else {
            (reset - now).num_milliseconds() as f64 / 1000.0
        }
    });
    (seconds / WEEKLY_BUCKET_SECONDS).floor()
}

pub fn at_hard_wall(engine: Engine, five_hour: f64, weekly: f64) -> bool {
    (engine_has_five_hour(engine) && five_hour >= FIVE_HOUR_HARD_PERCENT)
        || weekly >= WEEKLY_HARD_PERCENT
}

pub fn rotation_advice(engine: Engine, five_hour: f64, weekly: f64) -> RotationAdvice {
    if engine_has_five_hour(engine) && five_hour >= LIVE_ROTATION_FIVE_PERCENT {
        return RotationAdvice {
            rotate: true,
            reason: format!("5h {five_hour:.0}% at or above {LIVE_ROTATION_FIVE_PERCENT:.0}%"),
        };
    }
    if weekly >= LIVE_ROTATION_WEEKLY_PERCENT {
        return RotationAdvice {
            rotate: true,
            reason: format!("weekly {weekly:.0}% at or above {LIVE_ROTATION_WEEKLY_PERCENT:.0}%"),
        };
    }
    RotationAdvice {
        rotate: false,
        reason: if engine_has_five_hour(engine) {
            format!("headroom ok (5h {five_hour:.0}%, 7d {weekly:.0}%)")
        } else {
            format!("headroom ok (weekly {weekly:.0}%; {engine} has no 5h window)")
        },
    }
}

pub fn is_weekly_limit(window: Option<&str>) -> bool {
    let value = window.unwrap_or_default().to_ascii_lowercase();
    ["seven", "week", "7d", "day"]
        .iter()
        .any(|part| value.contains(part))
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    #[test]
    fn rotates_running_work_at_the_hard_wall() {
        assert!(rotation_advice(Engine::Claude, 99.0, 20.0).rotate);
        assert!(rotation_advice(Engine::Codex, 0.0, 99.0).rotate);
        assert!(!rotation_advice(Engine::Codex, 99.0, 20.0).rotate);
    }

    #[test]
    fn canonical_thresholds_are_shared_by_selection_and_rotation() {
        assert_eq!(FIVE_SKIP_PERCENT, FIVE_HOUR_SOFT_PERCENT);
        assert_eq!(FIVE_HARD_PERCENT, FIVE_HOUR_HARD_PERCENT);
        assert_eq!(WEEKLY_SKIP_PERCENT, WEEKLY_SOFT_PERCENT);
        assert_eq!(WEEKLY_HARD_PERCENT, WEEKLY_HARD_PERCENT);
        assert_eq!(ROTATE_FIVE_PERCENT, FIVE_HOUR_HARD_PERCENT);
        assert_eq!(ROTATE_WEEKLY_PERCENT, WEEKLY_HARD_PERCENT);
        assert_eq!(FIVE_HOUR_HARD_PERCENT, 99.0);
        assert_eq!(WEEKLY_HARD_PERCENT, 99.0);
        assert!(!at_hard_wall(Engine::Claude, 98.9, 98.9));
        assert!(at_hard_wall(Engine::Claude, 99.0, 0.0));
        assert!(at_hard_wall(Engine::Codex, 99.0, 99.0));
        assert!(!at_hard_wall(Engine::Codex, 99.0, 98.9));
    }

    #[test]
    fn reset_windows_zero_usage_and_expired_deadlines_use_unknown_horizon() {
        let now = Utc::now();
        assert_eq!(
            window_percent(Some(98.0), Some(now - Duration::seconds(1)), now),
            0.0
        );
        assert_eq!(
            window_percent(Some(98.0), Some(now + Duration::seconds(1)), now),
            98.0
        );
        assert_eq!(
            weekly_deadline_tier(Some(now + Duration::hours(5)), now),
            0.0
        );
        assert_eq!(
            weekly_deadline_tier(Some(now - Duration::seconds(1)), now),
            (WEEKLY_HORIZON_SECONDS / WEEKLY_BUCKET_SECONDS).floor()
        );
        assert_eq!(
            weekly_deadline_tier(None, now),
            (WEEKLY_HORIZON_SECONDS / WEEKLY_BUCKET_SECONDS).floor()
        );
    }

    #[test]
    fn quota_support_distinguishes_numeric_windows_from_reactive_signals() {
        assert_eq!(quota_support(Engine::Claude), QuotaSupport::Numeric);
        assert_eq!(quota_support(Engine::Codex), QuotaSupport::Numeric);
        for engine in [Engine::Opencode, Engine::Kimi, Engine::Grok] {
            assert_eq!(quota_support(engine), QuotaSupport::Reactive);
        }
    }
}
