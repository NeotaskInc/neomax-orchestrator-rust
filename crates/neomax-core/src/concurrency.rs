use serde::{Deserialize, Serialize};

use crate::{EffectiveSettings, Error, Result};

pub mod dispatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdmissionSnapshot {
    pub max_subagents: u32,
    pub active_subagents: u32,
}

impl AdmissionSnapshot {
    pub fn from_settings(settings: &EffectiveSettings, active_subagents: u32) -> Self {
        Self {
            max_subagents: settings.concurrency.max_subagents,
            active_subagents,
        }
    }

    pub fn available(self) -> u32 {
        self.max_subagents.saturating_sub(self.active_subagents)
    }

    pub fn admit(self, requested: u32) -> Result<()> {
        if requested == 0 {
            return Err(Error::InvalidArgument(
                "requested subagent count must be positive".into(),
            ));
        }
        if requested > self.available() {
            return Err(Error::Message(format!(
                "subagent limit reached: {} active + {} requested exceeds configured maximum {}",
                self.active_subagents, requested, self.max_subagents
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{ConcurrencySettings, SettingsFile};

    use super::*;

    fn settings(max_subagents: u32) -> EffectiveSettings {
        EffectiveSettings::resolve(
            SettingsFile {
                concurrency: ConcurrencySettings {
                    max_subagents,
                    ..ConcurrencySettings::default()
                },
                ..SettingsFile::default()
            },
            PathBuf::from("config.toml"),
            &Default::default(),
        )
        .unwrap()
    }

    #[test]
    fn enforces_the_global_limit_at_admission() {
        let admission = AdmissionSnapshot::from_settings(&settings(12), 10);
        assert_eq!(admission.available(), 2);
        assert!(admission.admit(2).is_ok());
        assert!(admission.admit(3).is_err());
    }
}
