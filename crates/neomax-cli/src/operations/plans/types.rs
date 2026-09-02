use neomax_core::scheduler::runtime::{RuntimeConfig, TickReport};
use neomax_core::scheduler::{PartState, PlanState};
use neomax_core::{EffectiveSettings, Engine, Error, Result};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlanRuntimeOptions {
    pub runtime: RuntimeConfig,
    pub max_ticks: usize,
    pub max_live_explicit: bool,
}

impl Default for PlanRuntimeOptions {
    fn default() -> Self {
        Self {
            runtime: RuntimeConfig::default(),
            max_ticks: 10_000,
            max_live_explicit: false,
        }
    }
}

impl PlanRuntimeOptions {
    pub(crate) fn resolve_run_all(
        self,
        settings: &EffectiveSettings,
        eligible_accounts: usize,
    ) -> Result<Self> {
        let runtime = if self.max_live_explicit {
            self.runtime
                .validate_run_all_against_settings(settings, eligible_accounts)?;
            self.runtime
        } else {
            let derived = RuntimeConfig::from_settings(settings, eligible_accounts)?;
            RuntimeConfig {
                max_live: derived.max_live,
                max_stall_cycles: self.runtime.max_stall_cycles,
                max_attempts: self.runtime.max_attempts,
            }
        };
        let resolved = Self { runtime, ..self };
        resolved.validate()?;
        Ok(resolved)
    }

    pub(crate) fn validate_against_settings(
        self,
        settings: &EffectiveSettings,
        eligible_accounts: Option<usize>,
    ) -> Result<()> {
        if self.max_live_explicit {
            self.runtime.validate_against_settings(settings)?;
            if let Some(eligible_accounts) = eligible_accounts {
                settings.validate_run_all_capacity(self.runtime.max_live, eligible_accounts)?;
            }
        } else {
            self.runtime.validate()?;
        }
        if self.max_ticks == 0 {
            return Err(Error::InvalidArgument(
                "scheduler max_ticks must be positive".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn validate(self) -> Result<()> {
        self.runtime.validate()?;
        if self.max_ticks == 0 {
            return Err(Error::InvalidArgument(
                "scheduler max_ticks must be positive".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PartStatusView {
    pub id: String,
    pub engine: Engine,
    pub status: PartState,
    pub run_id: Option<String>,
    pub branch: Option<String>,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct PlanRunReport {
    pub plan_id: String,
    pub status: String,
    pub finished: bool,
    pub ticks: usize,
    pub last_tick: Option<TickSummary>,
    pub state: PlanState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TickSummary {
    pub launched: Vec<String>,
    pub completed: Vec<String>,
    pub failed: Vec<String>,
    pub conflicted: Vec<String>,
    pub retried: Vec<String>,
    pub blocked: Vec<String>,
    pub stalled: bool,
    pub finished: bool,
}

impl From<&TickReport> for TickSummary {
    fn from(report: &TickReport) -> Self {
        Self {
            launched: report.launched.clone(),
            completed: report.completed.clone(),
            failed: report.failed.clone(),
            conflicted: report.conflicted.clone(),
            retried: report.retried.clone(),
            blocked: report.blocked.clone(),
            stalled: report.stalled,
            finished: report.finished,
        }
    }
}
