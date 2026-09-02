use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::atomic::{read_json_or_default, update_json_locked};
use crate::orchestration::rotation::normalize_profile_path;
use crate::Result;

const MAX_RESET_HORIZON_SECONDS: f64 = 7.0 * 24.0 * 3600.0 + 3600.0;

pub struct AccountControlStore {
    cooldowns: PathBuf,
    paused: PathBuf,
}

impl AccountControlStore {
    pub fn new(cooldowns: impl Into<PathBuf>, paused: impl Into<PathBuf>) -> Self {
        Self {
            cooldowns: cooldowns.into(),
            paused: paused.into(),
        }
    }

    pub fn cooldowns(&self) -> BTreeMap<String, f64> {
        read_json_or_default(&self.cooldowns)
    }

    pub fn cooldown_until(&self, profile: &Path, now: f64) -> Result<Option<f64>> {
        let key = normalized(profile)?.to_string_lossy().into_owned();
        Ok(self
            .cooldowns()
            .get(&key)
            .copied()
            .filter(|until| *until > now))
    }

    pub fn set_cooldown(
        &self,
        profile: &Path,
        reported_until: Option<f64>,
        now: f64,
        default_seconds: f64,
    ) -> Result<f64> {
        let key = normalized(profile)?.to_string_lossy().into_owned();
        let candidate = reported_until
            .filter(|value| value.is_finite() && *value > now)
            .unwrap_or(now + default_seconds)
            .clamp(now, now + MAX_RESET_HORIZON_SECONDS);
        let persisted_until = candidate.floor();
        let until = if persisted_until > now {
            persisted_until
        } else {
            (now + default_seconds)
                .clamp(now, now + MAX_RESET_HORIZON_SECONDS)
                .floor()
        };
        update_json_locked::<BTreeMap<String, f64>, _>(
            &self.cooldowns,
            &lock_path(&self.cooldowns),
            |state| {
                state.insert(key, until);
                Ok(())
            },
        )?;
        Ok(until)
    }

    pub fn clear_cooldown(&self, profile: &Path) -> Result<()> {
        let key = normalized(profile)?.to_string_lossy().into_owned();
        update_json_locked::<BTreeMap<String, f64>, _>(
            &self.cooldowns,
            &lock_path(&self.cooldowns),
            |state| {
                state.remove(&key);
                Ok(())
            },
        )?;
        Ok(())
    }

    pub fn paused(&self) -> BTreeSet<PathBuf> {
        let value: Value = read_json_or_default(&self.paused);
        paused_paths(&value)
    }

    pub fn is_paused(&self, profile: &Path) -> Result<bool> {
        Ok(self.paused().contains(&normalized_paused(profile)?))
    }

    pub fn set_paused(&self, profile: &Path, paused: bool) -> Result<()> {
        let profile = normalized_paused(profile)?;
        let lock = lock_path(&self.paused);
        update_json_locked::<Value, _>(&self.paused, &lock, |value| {
            let current = std::mem::take(value);
            *value = update_paused_state(current, &profile, paused);
            Ok(())
        })?;
        Ok(())
    }
}

fn paused_paths(value: &Value) -> BTreeSet<PathBuf> {
    match value {
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|raw| normalized_paused(Path::new(raw)).ok())
            .collect(),
        Value::Object(values) => values
            .keys()
            .filter_map(|raw| normalized_paused(Path::new(raw)).ok())
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn update_paused_state(value: Value, profile: &Path, paused: bool) -> Value {
    match value {
        Value::Array(values) => update_paused_array(values, profile, paused),
        Value::Object(values) => update_paused_object(values, profile, paused),
        _ => {
            if paused {
                Value::Array(vec![Value::String(profile.to_string_lossy().into_owned())])
            } else {
                Value::Array(Vec::new())
            }
        }
    }
}

fn update_paused_array(values: Vec<Value>, profile: &Path, paused: bool) -> Value {
    let mut unknown = Vec::new();
    let mut paths = BTreeSet::new();
    for value in values {
        let Some(raw) = value.as_str() else {
            unknown.push(value);
            continue;
        };
        let Ok(path) = normalized_paused(Path::new(raw)) else {
            unknown.push(value);
            continue;
        };
        if path != profile {
            paths.insert(path);
        }
    }
    if paused {
        paths.insert(profile.to_path_buf());
    }
    unknown.extend(
        paths
            .into_iter()
            .map(|path| Value::String(path.to_string_lossy().into_owned())),
    );
    Value::Array(unknown)
}

fn update_paused_object(
    values: serde_json::Map<String, Value>,
    profile: &Path,
    paused: bool,
) -> Value {
    let mut normalized_values = serde_json::Map::new();
    for (raw, value) in values {
        let Ok(path) = normalized_paused(Path::new(&raw)) else {
            normalized_values.insert(raw, value);
            continue;
        };
        if path != profile {
            normalized_values.insert(path.to_string_lossy().into_owned(), value);
        }
    }
    if paused {
        normalized_values.insert(profile.to_string_lossy().into_owned(), Value::Bool(true));
    }
    Value::Object(normalized_values)
}

fn normalized(path: &Path) -> Result<PathBuf> {
    if crate::io::is_rooted_but_not_absolute(path) {
        return Err(crate::Error::InvalidArgument(format!(
            "profile path must not be rooted without an absolute prefix: {}",
            path.display()
        )));
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn normalized_paused(path: &Path) -> Result<PathBuf> {
    if crate::io::is_rooted_but_not_absolute(path) {
        return Err(crate::Error::InvalidArgument(format!(
            "profile path must not be rooted without an absolute prefix: {}",
            path.display()
        )));
    }
    Ok(normalize_profile_path(path))
}

fn lock_path(path: &Path) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(".lock");
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn pause_and_cooldown_changes_are_atomic_and_live() {
        let temp = tempfile::tempdir().unwrap();
        let store = AccountControlStore::new(
            temp.path().join("cooldown.json"),
            temp.path().join("paused.json"),
        );
        let profile = temp.path().join("profile");
        store.set_paused(&profile, true).unwrap();
        assert!(store.is_paused(&profile).unwrap());
        store.set_paused(&profile, false).unwrap();
        assert!(!store.is_paused(&profile).unwrap());
        assert_eq!(
            store
                .set_cooldown(&profile, Some(1_500.9), 1_000.0, 300.0)
                .unwrap(),
            1_500.0
        );
        assert_eq!(
            store.cooldown_until(&profile, 1_100.0).unwrap(),
            Some(1_500.0)
        );
        store.clear_cooldown(&profile).unwrap();
        assert_eq!(store.cooldown_until(&profile, 1_100.0).unwrap(), None);
    }

    #[test]
    fn malformed_optional_control_state_degrades_to_empty() {
        let temp = tempfile::tempdir().unwrap();
        let cooldowns = temp.path().join("cooldown.json");
        let paused = temp.path().join("paused.json");
        fs::write(&cooldowns, "{").unwrap();
        fs::write(&paused, "{").unwrap();
        let store = AccountControlStore::new(cooldowns, paused);
        assert!(store.cooldowns().is_empty());
        assert!(store.paused().is_empty());
    }

    #[test]
    fn relative_legacy_paused_entries_match_canonical_profiles() {
        let temp = tempfile::tempdir().unwrap();
        let paused = temp.path().join("paused.json");
        let profile = normalize_profile_path("profiles/claude-1");
        fs::write(&paused, r#"["profiles/../profiles/claude-1"]"#).unwrap();
        let store = AccountControlStore::new(temp.path().join("cooldown.json"), &paused);

        let relative = PathBuf::from("profiles/../profiles/claude-1");
        assert!(store.is_paused(&relative).unwrap());
        assert!(store.is_paused(&profile).unwrap());
        assert_eq!(
            store.paused(),
            BTreeSet::from([normalize_profile_path(&profile)])
        );

        store.set_paused(&profile, false).unwrap();
        assert!(!store.is_paused(&profile).unwrap());
    }

    #[test]
    fn legacy_object_values_and_unknown_array_values_survive_pause_updates() {
        let temp = tempfile::tempdir().unwrap();
        let paused = temp.path().join("paused.json");
        let first = normalize_profile_path("profiles/claude-1");
        let second = normalize_profile_path("profiles/codex-2");
        fs::write(
            &paused,
            r#"{"profiles/../profiles/claude-1":{"reason":"manual"},"future-profile":{"future":true}}"#,
        )
        .unwrap();
        let store = AccountControlStore::new(temp.path().join("cooldown.json"), &paused);

        assert!(store.is_paused(&first).unwrap());
        store.set_paused(&second, true).unwrap();
        let updated: Value = serde_json::from_str(&fs::read_to_string(&paused).unwrap()).unwrap();
        let object = updated.as_object().unwrap();
        let future_profile = normalize_profile_path("future-profile");
        let first_profile = normalize_profile_path(&first);
        assert_eq!(
            object[future_profile.to_string_lossy().as_ref()],
            serde_json::json!({"future": true})
        );
        assert_eq!(
            object[first_profile.to_string_lossy().as_ref()],
            serde_json::json!({"reason": "manual"})
        );
        assert!(store.is_paused(&second).unwrap());

        fs::write(&paused, r#"["profiles/claude-1", {"future": true}, 7]"#).unwrap();
        store.set_paused(&first, false).unwrap();
        let updated: Value = serde_json::from_str(&fs::read_to_string(&paused).unwrap()).unwrap();
        assert_eq!(updated, serde_json::json!([{"future": true}, 7]));
    }

    #[test]
    fn past_or_current_reset_uses_the_default_cooldown() {
        let temp = tempfile::tempdir().unwrap();
        let store = AccountControlStore::new(
            temp.path().join("cooldown.json"),
            temp.path().join("paused.json"),
        );
        let profile = temp.path().join("profile");
        let until = store
            .set_cooldown(&profile, Some(999.0), 1_000.0, 300.0)
            .unwrap();
        assert_eq!(until, 1_300.0);
        assert_eq!(
            store.cooldown_until(&profile, 1_000.0).unwrap(),
            Some(1_300.0)
        );

        let until = store
            .set_cooldown(&profile, Some(1_000.0), 1_000.0, 600.0)
            .unwrap();
        assert_eq!(until, 1_600.0);
        assert_eq!(
            store.cooldown_until(&profile, 1_001.0).unwrap(),
            Some(1_600.0)
        );

        let until = store
            .set_cooldown(&profile, Some(1_000.5), 1_000.0, 120.0)
            .unwrap();
        assert_eq!(until, 1_120.0);
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_partial_root_profiles_without_rehoming_control_state() {
        let temp = tempfile::tempdir().unwrap();
        let cooldowns = temp.path().join("cooldown.json");
        let paused = temp.path().join("paused.json");
        let store = AccountControlStore::new(&cooldowns, &paused);

        for raw in [r"\rooted", r"C:drive-relative"] {
            let profile = Path::new(raw);
            assert!(store.set_paused(profile, true).is_err());
            assert!(store.set_cooldown(profile, None, 1_000.0, 300.0).is_err());
            assert_eq!(store.cooldown_until(profile, 1_000.0).unwrap_err().to_string(), format!(
                "invalid argument: profile path must not be rooted without an absolute prefix: {raw}"
            ));
            assert!(store.is_paused(profile).is_err());
        }

        fs::write(&paused, r#"["\\rooted", "C:drive-relative"]"#).unwrap();
        assert!(store.paused().is_empty());
    }
}
