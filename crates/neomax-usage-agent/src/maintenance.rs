use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use neomax_core::providers::scrub_provider_environment;
use neomax_core::runtime;
use serde::{Deserialize, Serialize};

use crate::config::AgentConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceAction {
    RotateTick,
    Keepalive,
    WorktreeTidy,
}

impl MaintenanceAction {
    pub const ALL: [Self; 3] = [Self::RotateTick, Self::Keepalive, Self::WorktreeTidy];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RotateTick => "rotate_tick",
            Self::Keepalive => "keepalive",
            Self::WorktreeTidy => "worktree_tidy",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenancePlan {
    pub action: MaintenanceAction,
    pub program: std::path::PathBuf,
    pub args: Vec<String>,
    pub timeout: Duration,
}

impl MaintenancePlan {
    pub fn for_action(config: &AgentConfig, action: MaintenanceAction) -> Self {
        let args = match action {
            MaintenanceAction::RotateTick => vec!["rotate-tick".into(), "--active".into()],
            MaintenanceAction::Keepalive => vec!["keepalive".into(), "--once".into()],
            MaintenanceAction::WorktreeTidy => {
                vec![
                    "tidy".into(),
                    "--automatic".into(),
                    "--any".into(),
                    "--json".into(),
                ]
            }
        };
        let timeout = match action {
            MaintenanceAction::WorktreeTidy => config.worktree_tidy_timeout,
            MaintenanceAction::RotateTick | MaintenanceAction::Keepalive => {
                config.maintenance_timeout
            }
        };
        Self {
            action,
            program: config.neomax_cli.clone(),
            args,
            timeout,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MaintenanceResult {
    pub action: MaintenanceAction,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub succeeded: bool,
}

pub trait MaintenanceExecutor: Send + Sync {
    fn execute(&self, plan: &MaintenancePlan) -> Result<MaintenanceResult>;
}

#[derive(Debug, Default)]
pub struct LocalMaintenanceExecutor;

impl MaintenanceExecutor for LocalMaintenanceExecutor {
    fn execute(&self, plan: &MaintenancePlan) -> Result<MaintenanceResult> {
        let current_dir = std::env::current_dir().unwrap_or_default();
        let mut command = runtime::process_command(&plan.program, &plan.args, &current_dir)?;
        scrub_provider_environment(&mut command);
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("start {} maintenance command", plan.action.as_str()))?;
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(MaintenanceResult {
                    action: plan.action,
                    exit_code: status.code(),
                    timed_out: false,
                    succeeded: status.success(),
                });
            }
            if start.elapsed() >= plan.timeout {
                let _ = child.kill();
                let status = child.wait().ok();
                return Ok(MaintenanceResult {
                    action: plan.action,
                    exit_code: status.and_then(|status| status.code()),
                    timed_out: true,
                    succeeded: false,
                });
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

#[cfg(test)]
mod tests;
