use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Duration, Utc};

use crate::Engine;
use crate::accounts::{QuotaSnapshot, QuotaTarget, QuotaWindow, quota_advice};

use super::support::QuotaFixture;

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
