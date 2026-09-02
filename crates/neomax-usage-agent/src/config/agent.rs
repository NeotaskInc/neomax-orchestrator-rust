use std::env;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use neomax_core::config::StatePaths;

use super::binary::resolve_required_binary;
use super::environment::ServiceEnvironment;
use super::paths::AgentPaths;
use super::validation::env_u64;
use super::{
    DEFAULT_KEEPALIVE_INTERVAL_SECS, DEFAULT_MAINTENANCE_TIMEOUT_SECS, DEFAULT_POLL_SECS,
    DEFAULT_RECENT_DAYS, DEFAULT_ROTATION_INTERVAL_SECS, DEFAULT_WORKTREE_TIDY_INTERVAL_SECS,
    DEFAULT_WORKTREE_TIDY_TIMEOUT_SECS,
};

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub paths: AgentPaths,
    pub poll_interval: Duration,
    pub recent_days: u32,
    pub rotation_interval: Duration,
    pub keepalive_interval: Duration,
    pub worktree_tidy_interval: Option<Duration>,
    pub worktree_tidy_timeout: Duration,
    pub maintenance_timeout: Duration,
    pub executable: PathBuf,
    pub neomax_cli: PathBuf,
    pub environment: ServiceEnvironment,
}

impl AgentConfig {
    pub fn discover() -> Result<Self> {
        let paths = AgentPaths::discover()?;
        Self::from_paths(paths)
    }

    pub fn discover_without_catalog() -> Result<Self> {
        Self::from_paths(AgentPaths::for_discovered_state(StatePaths::discover()?))
    }

    fn from_paths(paths: AgentPaths) -> Result<Self> {
        paths.validate()?;
        let poll_interval =
            Duration::from_secs(env_u64("NEOMAX_USAGE_POLL", DEFAULT_POLL_SECS, 1, 86_400)?);
        let recent_days = env_u64(
            "NEOMAX_USAGE_RECENT_DAYS",
            DEFAULT_RECENT_DAYS as u64,
            0,
            3650,
        )? as u32;
        let rotation_interval = Duration::from_secs(env_u64(
            "NEOMAX_ROTATE_TICK",
            DEFAULT_ROTATION_INTERVAL_SECS,
            1,
            86_400,
        )?);
        let keepalive_interval = Duration::from_secs(env_u64(
            "NEOMAX_KEEPALIVE_EVERY",
            DEFAULT_KEEPALIVE_INTERVAL_SECS,
            1,
            86_400,
        )?);
        let worktree_tidy_interval = match env_u64(
            "NEOMAX_WORKTREE_TIDY_EVERY",
            DEFAULT_WORKTREE_TIDY_INTERVAL_SECS,
            0,
            86_400,
        )? {
            0 => None,
            seconds => Some(Duration::from_secs(seconds)),
        };
        let worktree_tidy_timeout = Duration::from_secs(env_u64(
            "NEOMAX_WORKTREE_TIDY_TIMEOUT_SECS",
            DEFAULT_WORKTREE_TIDY_TIMEOUT_SECS,
            1,
            3_600,
        )?);
        let maintenance_timeout = Duration::from_secs(env_u64(
            "NEOMAX_MAINTENANCE_TIMEOUT_SECS",
            DEFAULT_MAINTENANCE_TIMEOUT_SECS,
            1,
            300,
        )?);
        let executable_input = env::var_os("NEOMAX_USAGE_AGENT_BIN")
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())
            .context("could not determine the usage-agent executable")?;
        let path = env::var_os("PATH");
        let executable = resolve_required_binary(
            "NEOMAX_USAGE_AGENT_BIN",
            executable_input.as_os_str(),
            path.as_deref(),
        )?;
        let neomax_cli_input = env::var_os("NEOMAX_CLI_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("neomax"));
        let neomax_cli = resolve_required_binary(
            "NEOMAX_CLI_BIN",
            neomax_cli_input.as_os_str(),
            path.as_deref(),
        )?;
        let environment = ServiceEnvironment::discover(&paths, &executable, &neomax_cli)?;
        Ok(Self {
            paths,
            poll_interval,
            recent_days,
            rotation_interval,
            keepalive_interval,
            worktree_tidy_interval,
            worktree_tidy_timeout,
            maintenance_timeout,
            executable,
            neomax_cli,
            environment,
        })
    }

    pub fn with_paths(paths: AgentPaths) -> Self {
        let executable = paths.home.join("bin").join(if cfg!(windows) {
            "neomax-usage-agent.exe"
        } else {
            "neomax-usage-agent"
        });
        let neomax_cli = paths.home.join("bin").join(if cfg!(windows) {
            "neomax.exe"
        } else {
            "neomax"
        });
        let environment = ServiceEnvironment::for_paths(&paths, &executable, &neomax_cli);
        Self {
            paths,
            poll_interval: Duration::from_secs(DEFAULT_POLL_SECS),
            recent_days: DEFAULT_RECENT_DAYS,
            rotation_interval: Duration::from_secs(DEFAULT_ROTATION_INTERVAL_SECS),
            keepalive_interval: Duration::from_secs(DEFAULT_KEEPALIVE_INTERVAL_SECS),
            worktree_tidy_interval: Some(Duration::from_secs(DEFAULT_WORKTREE_TIDY_INTERVAL_SECS)),
            worktree_tidy_timeout: Duration::from_secs(DEFAULT_WORKTREE_TIDY_TIMEOUT_SECS),
            maintenance_timeout: Duration::from_secs(DEFAULT_MAINTENANCE_TIMEOUT_SECS),
            executable,
            neomax_cli,
            environment,
        }
    }
}
