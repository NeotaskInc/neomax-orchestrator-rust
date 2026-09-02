use crate::settings::EffectiveSettings;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct AdmissionLimits {
    pub fleet_cap: Option<u32>,
    pub task_cap: u32,
    pub provider_cap: Option<u32>,
    pub lanes_per_account: u32,
    pub sessions_per_account: u32,
    pub lease_ttl_seconds: f64,
}

impl AdmissionLimits {
    pub fn from_settings(settings: &EffectiveSettings) -> Self {
        let fleet_cap = settings
            .concurrency
            .fleet_live_cap
            .unwrap_or(settings.concurrency.max_subagents)
            .min(settings.concurrency.max_subagents);
        Self {
            fleet_cap: Some(fleet_cap),
            task_cap: settings.concurrency.max_tasks,
            provider_cap: Some(settings.concurrency.max_subagents),
            lanes_per_account: settings.concurrency.lanes_per_account,
            sessions_per_account: settings.concurrency.max_sessions_per_account,
            lease_ttl_seconds: settings.concurrency.queue_ttl_seconds,
        }
    }

    pub(super) fn validate(&self) -> Result<()> {
        if self.lanes_per_account == 0 || self.sessions_per_account == 0 {
            return Err(Error::InvalidArgument(
                "dispatch admission account limits must be positive".into(),
            ));
        }
        if !self.lease_ttl_seconds.is_finite() || self.lease_ttl_seconds <= 0.0 {
            return Err(Error::InvalidArgument(
                "dispatch admission lease TTL must be finite and positive".into(),
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

    use super::AdmissionLimits;

    #[test]
    fn global_subagent_budget_bounds_cross_provider_fleet_admission() {
        let settings = crate::EffectiveSettings::resolve(
            SettingsFile {
                concurrency: ConcurrencySettings {
                    max_subagents: 7,
                    ..ConcurrencySettings::default()
                },
                ..SettingsFile::default()
            },
            PathBuf::from("config.toml"),
            &BTreeMap::new(),
        )
        .unwrap();

        let limits = AdmissionLimits::from_settings(&settings);
        assert_eq!(limits.fleet_cap, Some(7));
        assert_eq!(limits.provider_cap, Some(7));
    }

    #[test]
    fn explicit_fleet_cap_can_only_lower_the_global_subagent_budget() {
        let settings = crate::EffectiveSettings::resolve(
            SettingsFile {
                concurrency: ConcurrencySettings {
                    max_subagents: 7,
                    fleet_live_cap: Some(11),
                    ..ConcurrencySettings::default()
                },
                ..SettingsFile::default()
            },
            PathBuf::from("config.toml"),
            &BTreeMap::new(),
        )
        .unwrap();

        assert_eq!(AdmissionLimits::from_settings(&settings).fleet_cap, Some(7));
    }
}
