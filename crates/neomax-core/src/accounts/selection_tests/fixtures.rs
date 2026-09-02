use chrono::{DateTime, TimeZone, Utc};
use std::path::PathBuf;

use crate::Engine;

use super::super::AccountSnapshot;

pub(super) fn account(name: &str, five: f64, weekly: f64, live: u32) -> AccountSnapshot {
    AccountSnapshot {
        engine: Engine::Claude,
        account: name.into(),
        profile: PathBuf::from(name),
        binary_available: true,
        authenticated: true,
        rotation_eligible: false,
        paused: false,
        reserved: false,
        live_workers: live,
        five_hour_percent: Some(five),
        weekly_percent: Some(weekly),
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    }
}

pub(super) fn now() -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).single().unwrap()
}
