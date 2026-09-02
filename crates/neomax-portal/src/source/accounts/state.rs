use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use neomax_core::io::{LocalFileSource, ReadLimits, read_file};
use serde_json::Value;

const MAX_CONTROL_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) struct ControlState {
    cooldowns: BTreeMap<String, f64>,
    paused: BTreeSet<PathBuf>,
}

impl ControlState {
    pub(crate) fn load(cooldowns: &Path, paused: &Path) -> Self {
        let cooldowns = read_control(cooldowns)
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        let paused = read_control(paused)
            .map(|value| match value {
                Value::Array(values) => values
                    .into_iter()
                    .filter_map(|value| value.as_str().map(PathBuf::from))
                    .collect(),
                Value::Object(values) => values
                    .into_iter()
                    .map(|(path, _)| PathBuf::from(path))
                    .collect(),
                _ => BTreeSet::new(),
            })
            .unwrap_or_default();
        Self { cooldowns, paused }
    }

    pub(crate) fn is_paused(&self, profile: &Path) -> bool {
        self.paused.contains(profile)
    }

    pub(crate) fn cooldown_until(&self, profile: &Path, now: f64) -> Option<f64> {
        self.cooldowns
            .get(&profile.to_string_lossy().into_owned())
            .copied()
            .filter(|until| until.is_finite() && *until > now)
    }
}

fn read_control(path: &Path) -> Option<Value> {
    let max_bytes = usize::try_from(MAX_CONTROL_BYTES).ok()?;
    let bytes = read_file(
        &LocalFileSource,
        path,
        ReadLimits::new(max_bytes, std::time::Duration::from_secs(2)).ok()?,
    )
    .ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn oversized_control_state_degrades_without_an_unbounded_read() {
        let temp = tempfile::tempdir().unwrap();
        let cooldowns = temp.path().join("cooldowns.json");
        let paused = temp.path().join("paused.json");
        fs::write(&cooldowns, vec![b'x'; MAX_CONTROL_BYTES as usize + 1]).unwrap();
        fs::write(&paused, vec![b'x'; MAX_CONTROL_BYTES as usize + 1]).unwrap();
        let controls = ControlState::load(&cooldowns, &paused);
        assert!(controls.cooldowns.is_empty());
        assert!(controls.paused.is_empty());
    }
}
