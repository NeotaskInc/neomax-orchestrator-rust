use std::path::PathBuf;

use crate::accounts::AccountSnapshot;
use crate::Engine;

use super::super::{ProviderUsageCache, QuotaWindow};

pub(super) fn cache(five_hour_percent: f64) -> ProviderUsageCache {
    ProviderUsageCache {
        five_hour: QuotaWindow {
            used_percent: Some(five_hour_percent),
            ..QuotaWindow::default()
        },
        ..ProviderUsageCache::default()
    }
}

pub(super) fn account(engine: Engine, profile: PathBuf) -> AccountSnapshot {
    AccountSnapshot {
        engine,
        account: "1".into(),
        profile,
        binary_available: true,
        authenticated: true,
        rotation_eligible: true,
        paused: false,
        reserved: false,
        live_workers: 0,
        five_hour_percent: None,
        weekly_percent: None,
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    }
}
