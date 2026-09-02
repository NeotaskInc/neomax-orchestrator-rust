use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::accounts::{
    AccountRankingPolicy, AccountSnapshot, DEFAULT_WEEKLY_TIEBREAK_WEIGHT,
    LIVE_ROTATION_FIVE_PERCENT, WEEKLY_BUCKET_SECONDS, WEEKLY_HARD_PERCENT,
    WEEKLY_HORIZON_SECONDS,
};
use crate::orchestration::registry::OrchestratorRecord;

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorPolicy {
    pub hard_percent: f64,
    pub rotate_five_percent: f64,
    pub anti_stack_weight: f64,
    pub live_weight: f64,
    pub weekly_tiebreak_weight: f64,
    pub weekly_bucket_seconds: f64,
    pub weekly_horizon_seconds: f64,
}

impl Default for OrchestratorPolicy {
    fn default() -> Self {
        Self {
            hard_percent: WEEKLY_HARD_PERCENT,
            rotate_five_percent: LIVE_ROTATION_FIVE_PERCENT,
            anti_stack_weight: 100_000.0,
            live_weight: 100.0,
            weekly_tiebreak_weight: DEFAULT_WEEKLY_TIEBREAK_WEIGHT,
            weekly_bucket_seconds: WEEKLY_BUCKET_SECONDS,
            weekly_horizon_seconds: WEEKLY_HORIZON_SECONDS,
        }
    }
}

impl OrchestratorPolicy {
    pub fn account_ranking(&self) -> AccountRankingPolicy {
        AccountRankingPolicy {
            live_weight: self.live_weight,
            weekly_tiebreak_weight: self.weekly_tiebreak_weight,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeomaxChoice {
    pub engine: Engine,
    pub profile: PathBuf,
    pub pressure: Option<f64>,
    pub live: u32,
    pub worker_engines: Vec<Engine>,
    pub orchestrator_engines: Vec<Engine>,
    pub reason: String,
    pub priority: Vec<Engine>,
    pub cwd: PathBuf,
}

pub struct ProviderSelectionRequest<'a> {
    pub accounts: &'a [AccountSnapshot],
    pub orchestrators: &'a [OrchestratorRecord],
    pub engine: Engine,
    pub dedicated: bool,
    pub current_session: Option<&'a str>,
    pub now: DateTime<Utc>,
    pub policy: &'a OrchestratorPolicy,
}

pub struct NeomaxSelectionRequest<'a> {
    pub accounts: &'a [AccountSnapshot],
    pub orchestrators: &'a [OrchestratorRecord],
    pub priority: &'a [Engine],
    pub forced_engine: Option<Engine>,
    pub cwd: PathBuf,
    pub resume: bool,
    pub dedicated: bool,
    pub previous_engine: Option<Engine>,
    pub current_session: Option<&'a str>,
    pub now: DateTime<Utc>,
    pub policy: &'a OrchestratorPolicy,
}
