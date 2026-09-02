mod eligibility;
mod live_source;
mod policy_selection;

use neomax_core::Engine;
use neomax_core::accounts::AccountSnapshot;

pub(super) fn account(
    engine: Engine,
    name: &str,
    profile: &str,
    five: f64,
    weekly: f64,
) -> AccountSnapshot {
    AccountSnapshot {
        engine,
        account: name.into(),
        profile: profile.into(),
        binary_available: true,
        authenticated: true,
        rotation_eligible: true,
        paused: false,
        reserved: false,
        live_workers: 0,
        five_hour_percent: Some(five),
        weekly_percent: Some(weekly),
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    }
}
