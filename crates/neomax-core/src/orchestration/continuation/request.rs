use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::Engine;
use crate::accounts::AccountSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationTrigger {
    Quota,
    Manual,
    Tick,
}

impl RotationTrigger {
    pub const fn allows_cross_provider(self) -> bool {
        matches!(self, Self::Quota | Self::Tick)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Quota => "quota",
            Self::Manual => "manual",
            Self::Tick => "tick",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContinuationRequest {
    pub run_id: String,
    pub engine: Engine,
    pub source_profile: PathBuf,
    pub source_account: String,
    pub source_rotation_eligible: bool,
    pub target: AccountSnapshot,
    pub trigger: RotationTrigger,
    pub reason: String,
    pub now: DateTime<Utc>,
    pub cwd: PathBuf,
    pub workdir: PathBuf,
    pub repo: Option<PathBuf>,
    pub worktree: Option<PathBuf>,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub project: Option<String>,
    pub session: Option<String>,
    pub prompt: String,
    pub attempt: u32,
    /// Latest observed five-hour usage. `None` means no numeric observation
    /// exists for this account or provider.
    pub five_hour: Option<f64>,
    /// Latest observed seven-day usage. `None` means no numeric observation
    /// exists for this account or provider.
    pub seven_day: Option<f64>,
    pub resets_at: Option<f64>,
    pub limit_window: Option<String>,
    pub run_state: BTreeMap<String, Value>,
}

impl ContinuationRequest {
    pub fn from_run(
        run: &crate::runs::RunRecord,
        target: AccountSnapshot,
        trigger: RotationTrigger,
        now: DateTime<Utc>,
    ) -> Self {
        Self::from_run_with_source_eligibility(run, target, trigger, now, true)
    }

    pub fn from_run_with_source_eligibility(
        run: &crate::runs::RunRecord,
        target: AccountSnapshot,
        trigger: RotationTrigger,
        now: DateTime<Utc>,
        source_rotation_eligible: bool,
    ) -> Self {
        let run_state = serde_json::to_value(run)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .map(|object| object.into_iter().collect())
            .unwrap_or_default();
        Self {
            run_id: run.id.clone(),
            engine: run.engine,
            source_profile: run.profile.clone(),
            source_account: run.account(),
            source_rotation_eligible,
            target,
            trigger,
            reason: run
                .error_detail
                .clone()
                .unwrap_or_else(|| "quota rotation".into()),
            now,
            cwd: run.cwd.clone().unwrap_or_else(|| run.workdir.clone()),
            workdir: run.workdir.clone(),
            repo: run.repo.clone(),
            worktree: run.worktree.clone(),
            branch: run.branch.clone(),
            base: run.base.clone(),
            project: run.project.clone(),
            session: run.session.clone(),
            prompt: run.prompt.clone(),
            attempt: run.attempt,
            five_hour: None,
            seven_day: None,
            resets_at: run.resets_at,
            limit_window: run.limit_window.clone(),
            run_state,
        }
    }

    pub fn with_observed_quota(mut self, source: Option<&AccountSnapshot>) -> Self {
        self.five_hour = source
            .filter(|account| account.engine == self.engine)
            .filter(|account| crate::accounts::engine_has_five_hour(account.engine))
            .and_then(|account| account.five_hour_percent)
            .filter(|value| value.is_finite());
        self.seven_day = source
            .filter(|account| account.engine == self.engine)
            .and_then(|account| account.weekly_percent)
            .filter(|value| value.is_finite());
        self
    }
}
