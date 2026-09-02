use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::Result;
use crate::accounts::AccountControlStore;
use crate::runs::{RunRecord, RunStatus};

pub fn record_limit_cooldown(
    controls: &AccountControlStore,
    run: &RunRecord,
    status: RunStatus,
    now: DateTime<Utc>,
    default: Duration,
) -> Result<Option<f64>> {
    if status != RunStatus::Limit {
        return Ok(None);
    }
    controls
        .set_cooldown(
            &run.profile,
            run.resets_at,
            now.timestamp_millis() as f64 / 1000.0,
            default.as_secs_f64(),
        )
        .map(Some)
}
