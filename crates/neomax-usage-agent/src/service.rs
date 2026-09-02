use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use neomax_core::atomic::with_exclusive_lock;
use serde::Serialize;

use crate::collector::{SweepReport, UsageCollector};
use crate::config::AgentConfig;
use crate::maintenance::{
    LocalMaintenanceExecutor, MaintenanceAction, MaintenanceExecutor, MaintenancePlan,
    MaintenanceResult,
};
use crate::quota::{LocalQuotaRefresher, QuotaRefresher, QuotaReport};
use crate::state::{MaintenanceSummary, WatchState};

#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    pub rebuild: bool,
    pub no_backfill: bool,
    pub once: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub bootstrap: Option<SweepReport>,
    pub sweep: SweepReport,
    pub baselined: bool,
    pub quota: QuotaReport,
    pub maintenance: Vec<MaintenanceReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaintenanceReport {
    pub action: MaintenanceAction,
    pub attempted_at: i64,
    pub completed_at: i64,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub succeeded: bool,
    pub error: Option<String>,
}

pub struct WatchService {
    config: AgentConfig,
    collector: UsageCollector,
    quota: std::sync::Arc<dyn QuotaRefresher>,
    maintenance: std::sync::Arc<dyn MaintenanceExecutor>,
}

impl WatchService {
    pub fn new(config: AgentConfig) -> Self {
        let collector = UsageCollector::new(config.paths.clone());
        let quota = std::sync::Arc::new(LocalQuotaRefresher::new(config.paths.clone()));
        Self {
            config,
            collector,
            quota,
            maintenance: std::sync::Arc::new(LocalMaintenanceExecutor),
        }
    }

    pub fn with_collector(config: AgentConfig, collector: UsageCollector) -> Self {
        let quota = std::sync::Arc::new(LocalQuotaRefresher::new(config.paths.clone()));
        Self {
            config,
            collector,
            quota,
            maintenance: std::sync::Arc::new(LocalMaintenanceExecutor),
        }
    }

    pub fn with_maintenance(
        config: AgentConfig,
        collector: UsageCollector,
        maintenance: std::sync::Arc<dyn MaintenanceExecutor>,
    ) -> Self {
        let quota = std::sync::Arc::new(LocalQuotaRefresher::new(config.paths.clone()));
        Self {
            config,
            collector,
            quota,
            maintenance,
        }
    }

    pub fn with_quota(
        config: AgentConfig,
        collector: UsageCollector,
        quota: std::sync::Arc<dyn QuotaRefresher>,
    ) -> Self {
        Self {
            config,
            collector,
            quota,
            maintenance: std::sync::Arc::new(LocalMaintenanceExecutor),
        }
    }

    pub fn with_components(
        config: AgentConfig,
        collector: UsageCollector,
        quota: std::sync::Arc<dyn QuotaRefresher>,
        maintenance: std::sync::Arc<dyn MaintenanceExecutor>,
    ) -> Self {
        Self {
            config,
            collector,
            quota,
            maintenance,
        }
    }

    pub fn run_once(&self, options: RunOptions) -> Result<RunReport> {
        let state_path = self.collector.state_path().to_path_buf();
        let lock_path = std::path::PathBuf::from(format!("{}.lock", state_path.display()));
        let mut report = with_exclusive_lock(&lock_path, || {
            self.run_once_locked(&state_path, options)
                .map_err(|error| neomax_core::Error::Message(error.to_string()))
        })?;
        report.quota = if has_rate_limit(&report) {
            self.quota.refresh_after_rate_limit()?
        } else {
            self.quota.refresh(false)?
        };
        report.maintenance = self.run_maintenance()?;
        Ok(report)
    }

    fn run_once_locked(
        &self,
        state_path: &std::path::Path,
        options: RunOptions,
    ) -> Result<RunReport> {
        let ledger_path = self.config.paths.state.usage_ledger.clone();
        let mut state = WatchState::load(state_path)?;
        let mut bootstrap = None;
        if options.rebuild {
            WatchState::clear_ledger(&ledger_path)?;
            state.reset();
        }
        if !state.baselined {
            let mode = if options.no_backfill {
                crate::collector::SweepMode::Baseline
            } else {
                crate::collector::SweepMode::Full
            };
            bootstrap = Some(
                self.collector
                    .sweep(&mut state, mode, self.config.recent_days)?,
            );
            state.baselined = true;
        } else if !WatchState::has_ledger(&ledger_path)?
            && !options.no_backfill
            && self.collector.has_sources()
        {
            state.reset();
            bootstrap = Some(self.collector.sweep(
                &mut state,
                crate::collector::SweepMode::Full,
                self.config.recent_days,
            )?);
            state.baselined = true;
        }
        let sweep = self.collector.sweep(
            &mut state,
            crate::collector::SweepMode::Incremental,
            self.config.recent_days,
        )?;
        state.compact();
        state.save(state_path)?;
        Ok(RunReport {
            bootstrap,
            sweep,
            baselined: state.baselined,
            quota: QuotaReport::default(),
            maintenance: Vec::new(),
        })
    }

    fn run_maintenance(&self) -> Result<Vec<MaintenanceReport>> {
        let now = Utc::now().timestamp();
        let actions = self.claim_due_actions(now)?;
        let mut reports = Vec::with_capacity(actions.len());
        for action in actions {
            let plan = MaintenancePlan::for_action(&self.config, action);
            let outcome = self.maintenance.execute(&plan);
            let completed_at = Utc::now().timestamp();
            let report = match outcome {
                Ok(result) => report_from_result(result, now, completed_at),
                Err(_) => MaintenanceReport {
                    action,
                    attempted_at: now,
                    completed_at,
                    exit_code: None,
                    timed_out: false,
                    succeeded: false,
                    error: Some(format!("{} command could not start", action.as_str())),
                },
            };
            self.record_maintenance(&report)?;
            reports.push(report);
        }
        Ok(reports)
    }

    fn claim_due_actions(&self, now: i64) -> Result<Vec<MaintenanceAction>> {
        let state_path = self.collector.state_path().to_path_buf();
        let lock_path = std::path::PathBuf::from(format!("{}.lock", state_path.display()));
        let actions = with_exclusive_lock(&lock_path, || {
            let mut state = WatchState::load(&state_path)
                .map_err(|error| neomax_core::Error::Message(error.to_string()))?;
            let mut actions = Vec::new();
            if due(
                state.maintenance.last_rotation_attempt,
                now,
                Some(self.config.rotation_interval),
            ) {
                state.maintenance.last_rotation_attempt = Some(now);
                actions.push(MaintenanceAction::RotateTick);
            }
            if due(
                state.maintenance.last_keepalive_attempt,
                now,
                Some(self.config.keepalive_interval),
            ) {
                state.maintenance.last_keepalive_attempt = Some(now);
                actions.push(MaintenanceAction::Keepalive);
            }
            if due(
                state.maintenance.last_worktree_tidy_attempt,
                now,
                self.config.worktree_tidy_interval,
            ) {
                state.maintenance.last_worktree_tidy_attempt = Some(now);
                actions.push(MaintenanceAction::WorktreeTidy);
            }
            state
                .save(&state_path)
                .map_err(|error| neomax_core::Error::Message(error.to_string()))?;
            Ok(actions)
        })?;
        Ok(actions)
    }

    fn record_maintenance(&self, report: &MaintenanceReport) -> Result<()> {
        let state_path = self.collector.state_path().to_path_buf();
        let lock_path = std::path::PathBuf::from(format!("{}.lock", state_path.display()));
        with_exclusive_lock(&lock_path, || {
            let mut state = WatchState::load(&state_path)
                .map_err(|error| neomax_core::Error::Message(error.to_string()))?;
            let summary = MaintenanceSummary {
                attempted_at: report.attempted_at,
                completed_at: Some(report.completed_at),
                exit_code: report.exit_code,
                timed_out: report.timed_out,
                succeeded: report.succeeded,
            };
            match report.action {
                MaintenanceAction::RotateTick => state.maintenance.last_rotation = Some(summary),
                MaintenanceAction::Keepalive => state.maintenance.last_keepalive = Some(summary),
                MaintenanceAction::WorktreeTidy => {
                    state.maintenance.last_worktree_tidy = Some(summary)
                }
            }
            state
                .save(&state_path)
                .map_err(|error| neomax_core::Error::Message(error.to_string()))?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn run_forever(&self, options: RunOptions) -> Result<()> {
        let _ = self.run_once(options)?;
        let steady_state = RunOptions::default();
        loop {
            thread::sleep(self.config.poll_interval);
            self.run_once(steady_state).context("usage watch sweep")?;
        }
    }

    pub fn poll_interval(&self) -> Duration {
        self.config.poll_interval
    }
}

fn due(last: Option<i64>, now: i64, interval: Option<Duration>) -> bool {
    interval.is_some_and(|interval| {
        if interval.is_zero() {
            return false;
        }
        last.is_none_or(|previous| now.saturating_sub(previous) >= interval.as_secs() as i64)
    })
}

fn report_from_result(
    result: MaintenanceResult,
    attempted_at: i64,
    completed_at: i64,
) -> MaintenanceReport {
    MaintenanceReport {
        action: result.action,
        attempted_at,
        completed_at,
        exit_code: result.exit_code,
        timed_out: result.timed_out,
        succeeded: result.succeeded && result.exit_code == Some(0) && !result.timed_out,
        error: None,
    }
}

fn has_rate_limit(report: &RunReport) -> bool {
    report.sweep.rate_limits > 0
        || report
            .bootstrap
            .as_ref()
            .is_some_and(|sweep| sweep.rate_limits > 0)
}
