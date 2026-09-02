use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::runs::{ArchiveOutcome, RunStatus};

#[derive(Debug, Clone)]
pub struct FinalizeOptions {
    pub now: DateTime<Utc>,
    pub account_number: Option<u32>,
    pub default_cooldown: Duration,
}

impl FinalizeOptions {
    pub fn now(now: DateTime<Utc>) -> Self {
        Self {
            now,
            account_number: None,
            default_cooldown: Duration::from_secs(30 * 60),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finalization {
    pub status: RunStatus,
    pub exit_code: i32,
    pub cooldown_until: Option<f64>,
    pub archive: Option<ArchiveOutcome>,
    pub warnings: Vec<String>,
}

pub const fn exit_code(status: RunStatus) -> i32 {
    match status {
        RunStatus::Done | RunStatus::Integrated => 0,
        RunStatus::Limit => 75,
        RunStatus::Aborted | RunStatus::Interrupted => 130,
        RunStatus::Stalled | RunStatus::Timeout => 124,
        RunStatus::Error | RunStatus::Orphaned | RunStatus::Unknown | RunStatus::Running => 1,
    }
}
