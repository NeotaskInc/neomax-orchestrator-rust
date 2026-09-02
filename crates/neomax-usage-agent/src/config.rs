mod agent;
mod binary;
mod environment;
mod paths;
mod validation;

#[cfg(test)]
mod tests;

pub const DEFAULT_POLL_SECS: u64 = 3;
pub const DEFAULT_RECENT_DAYS: u32 = 2;
pub const DEFAULT_ROTATION_INTERVAL_SECS: u64 = 30;
pub const DEFAULT_KEEPALIVE_INTERVAL_SECS: u64 = 8 * 60;
pub const DEFAULT_WORKTREE_TIDY_INTERVAL_SECS: u64 = 10 * 60;
pub const DEFAULT_WORKTREE_TIDY_TIMEOUT_SECS: u64 = 5 * 60;
pub const DEFAULT_MAINTENANCE_TIMEOUT_SECS: u64 = 30;
pub const SERVICE_LABEL: &str = "io.neomax.usagewatch";
#[cfg(target_os = "macos")]
pub const LEGACY_SERVICE_LABEL: &str = "io.cmax.usagewatch";
pub use agent::AgentConfig;
pub use environment::ServiceEnvironment;
pub use paths::AgentPaths;
