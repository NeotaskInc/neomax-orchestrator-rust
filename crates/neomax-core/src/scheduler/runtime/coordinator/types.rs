use crate::{EffectiveSettings, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub max_live: usize,
    pub max_stall_cycles: usize,
    pub max_attempts: u32,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_live: 1,
            max_stall_cycles: 360,
            max_attempts: 3,
        }
    }
}

impl RuntimeConfig {
    /// Builds the scheduler's run-all defaults from the already-resolved
    /// settings. Keeping this derivation here prevents the CLI and detached
    /// supervisor from inventing separate concurrency defaults.
    pub fn from_settings(settings: &EffectiveSettings, eligible_accounts: usize) -> Result<Self> {
        let max_live = settings.default_run_all_capacity(eligible_accounts);
        if max_live == 0 {
            return Err(Error::InvalidArgument(
                "scheduler run-all capacity is zero; worker dispatch is disabled".into(),
            ));
        }
        let config = Self {
            max_live,
            ..Self::default()
        };
        config.validate_run_all_against_settings(settings, eligible_accounts)?;
        Ok(config)
    }

    /// Validates a command-line max-live value against the global effective
    /// subagent budget. This is intentionally an error rather than a silent
    /// clamp so an operator can see that the requested capacity was rejected.
    pub fn validate_against_settings(self, settings: &EffectiveSettings) -> Result<()> {
        self.validate()?;
        let mut static_capacity = settings.concurrency.max_subagents as usize;
        if settings.concurrency.max_tasks != 0 {
            static_capacity = static_capacity.min(settings.concurrency.max_tasks as usize);
        }
        if let Some(fleet_cap) = settings.concurrency.fleet_live_cap {
            static_capacity = static_capacity.min(fleet_cap as usize);
        }
        if self.max_live > static_capacity {
            return Err(Error::InvalidArgument(format!(
                concat!(
                    "scheduler max_live {} exceeds effective static capacity {} ",
                    "(max_subagents={}, max_tasks={}, fleet_live_cap={})"
                ),
                self.max_live,
                static_capacity,
                settings.concurrency.max_subagents,
                settings.concurrency.max_tasks,
                settings
                    .concurrency
                    .fleet_live_cap
                    .map_or_else(|| "none".to_owned(), |value| value.to_string()),
            )));
        }
        Ok(())
    }

    pub fn validate_run_all_against_settings(
        self,
        settings: &EffectiveSettings,
        eligible_accounts: usize,
    ) -> Result<()> {
        self.validate_against_settings(settings)?;
        settings.validate_run_all_capacity(self.max_live, eligible_accounts)
    }

    pub fn validate(self) -> Result<()> {
        if self.max_live == 0 {
            return Err(Error::InvalidArgument(
                "scheduler max_live must be positive".into(),
            ));
        }
        if self.max_stall_cycles == 0 {
            return Err(Error::InvalidArgument(
                "scheduler max_stall_cycles must be positive".into(),
            ));
        }
        if self.max_attempts == 0 {
            return Err(Error::InvalidArgument(
                "scheduler max_attempts must be positive".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::{ConcurrencySettings, SettingsFile};

    use super::*;

    fn settings(max_subagents: u32, lanes_per_account: u32) -> EffectiveSettings {
        EffectiveSettings::resolve(
            SettingsFile {
                concurrency: ConcurrencySettings {
                    max_subagents,
                    lanes_per_account,
                    ..ConcurrencySettings::default()
                },
                ..SettingsFile::default()
            },
            PathBuf::from("config.toml"),
            &BTreeMap::new(),
        )
        .unwrap()
    }

    #[test]
    fn run_all_runtime_defaults_are_derived_from_effective_settings() {
        let config = RuntimeConfig::from_settings(&settings(8, 3), 4).unwrap();
        assert_eq!(config.max_live, 8);
    }

    #[test]
    fn explicit_max_live_cannot_exceed_effective_subagent_budget() {
        let error = RuntimeConfig {
            max_live: 9,
            ..RuntimeConfig::default()
        }
        .validate_against_settings(&settings(8, 3))
        .unwrap_err();
        assert!(error.to_string().contains("max_subagents"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickReport {
    pub launched: Vec<String>,
    pub completed: Vec<String>,
    pub failed: Vec<String>,
    pub conflicted: Vec<String>,
    pub retried: Vec<String>,
    pub blocked: Vec<String>,
    pub stalled: bool,
    pub finished: bool,
}

impl TickReport {
    pub(super) fn empty() -> Self {
        Self {
            launched: Vec::new(),
            completed: Vec::new(),
            failed: Vec::new(),
            conflicted: Vec::new(),
            retried: Vec::new(),
            blocked: Vec::new(),
            stalled: false,
            finished: false,
        }
    }

    pub(super) fn progressed(&self) -> bool {
        !self.launched.is_empty()
            || !self.completed.is_empty()
            || !self.failed.is_empty()
            || !self.conflicted.is_empty()
            || !self.retried.is_empty()
            || !self.blocked.is_empty()
    }
}
