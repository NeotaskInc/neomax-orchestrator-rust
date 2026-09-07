use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Duration, Utc};

use crate::Engine;
use crate::accounts::{QuotaSnapshot, QuotaTarget, QuotaWindow, quota_advice};

use super::support::QuotaFixture;

#[test]
fn live_monitor_rotates_only_the_model_family_at_its_weekly_wall() {
    let now = Utc::now();
    let profile = PathBuf::from("/profiles/claude1");
    let reset = now + Duration::days(2);
    let target = QuotaTarget { engine: Engine::Claude, profile: profile.clone() };
    let mut quota = QuotaFixture { snapshots: BTreeMap::from([((Engine::Claude, profile.clone()), QuotaSnapshot {
        available: true, five_hour_percent:Some(10.0), weekly_percent:Some(40.0),
        model_weekly: BTreeMap::from([("fable".into(), crate::accounts::ModelQuotaWindow {used_percent:Some(100.0),resets_at:Some(reset.timestamp() as f64)})]),
        ..QuotaSnapshot::default()
    })]) };
    let advice = crate::accounts::quota_advice_for_model(&quota, &target, "claude-fable-5-1[1m]", now);
    assert!(advice.rotate);
    assert_eq!(advice.limit_window.unwrap().as_str(), "seven_day_overage_included");
    assert_eq!(advice.resets_at.unwrap().timestamp(), reset.timestamp());
    assert!(!crate::accounts::quota_advice_for_model(&quota, &target, "claude-opus-5", now).rotate);
    quota.snapshots.get_mut(&(Engine::Claude, profile)).unwrap().weekly_percent = Some(99.0);
    for model in ["claude-opus-5", "claude-fable-5"] {
        let advice = crate::accounts::quota_advice_for_model(&quota, &target, model, now);
        assert_eq!(advice.limit_window, Some(QuotaWindow::Weekly));
    }
}

#[test]
fn converts_a_cached_hard_wall_into_a_live_rotation_directive() {
    let now = Utc::now();
    let profile = PathBuf::from("/profiles/claude1");
    let reset = now + Duration::hours(1);
    let quota = QuotaFixture {
        snapshots: BTreeMap::from([(
            (Engine::Claude, profile.clone()),
            QuotaSnapshot {
                available: true,
                five_hour_percent: Some(99.0),
                five_hour_reset_at: Some(reset),
                ..QuotaSnapshot::default()
            },
        )]),
    };
    let advice = quota_advice(
        &quota,
        &QuotaTarget {
            engine: Engine::Claude,
            profile: profile.clone(),
        },
        now,
    );
    assert!(advice.rotate);
    assert_eq!(advice.limit_window, Some(QuotaWindow::FiveHour));
    assert_eq!(advice.resets_at, Some(reset));
}
