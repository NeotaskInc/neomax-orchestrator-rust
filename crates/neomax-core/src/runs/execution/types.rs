use std::path::PathBuf;
use std::time::Duration;

use crate::providers::ParsedEvents;
use crate::runs::RunStatus;
use crate::{Error, Result};

/// The largest supported wall or stall timeout in minutes.
pub const MAX_TIMEOUT_MINUTES: f64 = 365.0 * 24.0 * 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KilledFor {
    Timeout,
    Stalled,
    Quota,
    Aborted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaRotation {
    pub reason: String,
    pub resets_at: Option<f64>,
    pub limit_window: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SupervisorDirective {
    #[default]
    Continue,
    Rotate(QuotaRotation),
    Abort,
}

#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    pub wall_timeout: Option<Duration>,
    pub stall_timeout: Option<Duration>,
    pub poll_interval: Duration,
    pub terminate_grace: Duration,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            wall_timeout: Some(Duration::from_secs(240 * 60)),
            stall_timeout: Some(Duration::from_secs(30 * 60)),
            poll_interval: Duration::from_secs(2),
            terminate_grace: Duration::from_secs(2),
        }
    }
}

impl SupervisorConfig {
    pub fn for_run(run: &crate::runs::RunRecord) -> Result<Self> {
        Ok(Self {
            wall_timeout: minutes(run.wall_min, 240.0, "wall_min")?,
            stall_timeout: minutes(
                run.stall_min,
                if run.ultra { 60.0 } else { 30.0 },
                "stall_min",
            )?,
            ..Self::default()
        })
    }
}

#[derive(Debug, Clone)]
pub struct AttemptOutcome {
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub parsed: ParsedEvents,
    pub stderr_tail: String,
    pub log_path: PathBuf,
    pub stderr_path: PathBuf,
}

fn minutes(value: Option<f64>, fallback: f64, name: &str) -> Result<Option<Duration>> {
    let value = value.unwrap_or(fallback);
    if !value.is_finite() || !(0.0..=MAX_TIMEOUT_MINUTES).contains(&value) {
        return Err(Error::InvalidArgument(format!(
            "{name} must be between 0 and {MAX_TIMEOUT_MINUTES} minutes"
        )));
    }
    if value == 0.0 {
        return Ok(None);
    }
    Duration::try_from_secs_f64(value * 60.0)
        .map(Some)
        .map_err(|error| Error::InvalidArgument(format!("{name} is not a valid timeout: {error}")))
}

#[cfg(test)]
mod tests {
    use crate::runs::RunRecord;

    use super::*;

    fn run(value: serde_json::Value) -> RunRecord {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn resolves_default_ultra_custom_and_disabled_deadlines() {
        let regular = SupervisorConfig::for_run(&run(serde_json::json!({
            "id":"regular", "engine":"codex", "status":"running", "started":1
        })))
        .unwrap();
        assert_eq!(regular.wall_timeout, Some(Duration::from_secs(240 * 60)));
        assert_eq!(regular.stall_timeout, Some(Duration::from_secs(30 * 60)));

        let ultra = SupervisorConfig::for_run(&run(serde_json::json!({
            "id":"ultra", "engine":"codex", "status":"running", "started":1,
            "ultra":true
        })))
        .unwrap();
        assert_eq!(ultra.stall_timeout, Some(Duration::from_secs(60 * 60)));

        let custom = SupervisorConfig::for_run(&run(serde_json::json!({
            "id":"custom", "engine":"codex", "status":"running", "started":1,
            "wall_min":1.5, "stall_min":2.5
        })))
        .unwrap();
        assert_eq!(custom.wall_timeout, Some(Duration::from_secs(90)));
        assert_eq!(custom.stall_timeout, Some(Duration::from_secs(150)));

        let disabled = SupervisorConfig::for_run(&run(serde_json::json!({
            "id":"disabled", "engine":"codex", "status":"running", "started":1,
            "wall_min":0, "stall_min":0
        })))
        .unwrap();
        assert_eq!(disabled.wall_timeout, None);
        assert_eq!(disabled.stall_timeout, None);
    }

    #[test]
    fn accepts_the_timeout_ceiling_without_overflow() {
        let mut run = run(serde_json::json!({
            "id":"ceiling", "engine":"codex", "status":"running", "started":1
        }));
        run.wall_min = Some(MAX_TIMEOUT_MINUTES);
        run.stall_min = Some(MAX_TIMEOUT_MINUTES);
        let config = SupervisorConfig::for_run(&run).unwrap();
        assert_eq!(
            config.wall_timeout,
            Some(Duration::from_secs(365 * 24 * 60 * 60))
        );
        assert_eq!(config.wall_timeout, config.stall_timeout);
    }

    #[test]
    fn rejects_unrepresentable_persisted_timeouts_with_clear_errors() {
        for (name, value) in [
            ("wall_min", f64::MAX),
            ("wall_min", f64::INFINITY),
            ("wall_min", f64::NAN),
            ("stall_min", -1.0),
            ("stall_min", MAX_TIMEOUT_MINUTES + 1.0),
        ] {
            let mut run = run(serde_json::json!({
                "id":"invalid", "engine":"codex", "status":"running", "started":1
            }));
            if name == "wall_min" {
                run.wall_min = Some(value);
            } else {
                run.stall_min = Some(value);
            }
            let error = SupervisorConfig::for_run(&run).unwrap_err();
            assert!(error.to_string().contains(name));
            assert!(error.to_string().contains("between 0 and"));
        }
    }
}
