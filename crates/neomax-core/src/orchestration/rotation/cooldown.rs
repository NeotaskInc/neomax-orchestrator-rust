use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Number, Value};

use crate::config::StatePaths;
use crate::Result;
use crate::atomic::{read_json, update_json_locked, with_exclusive_lock, write_json_atomic};

#[derive(Debug, Clone)]
pub struct AccountCooldownStore {
    path: PathBuf,
    lock: PathBuf,
}

impl AccountCooldownStore {
    pub fn new(path: impl Into<PathBuf>, lock: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: lock.into(),
        }
    }

    pub fn in_state_dir(state: impl AsRef<Path>) -> Self {
        Self::in_paths(&StatePaths::new(
            PathBuf::new(),
            state.as_ref().to_path_buf(),
        ))
    }

    pub fn in_paths(paths: &StatePaths) -> Self {
        Self::new(
            paths.account_cooldown.clone(),
            paths.account_cooldown_lock.clone(),
        )
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cooldowns(&self) -> BTreeMap<String, i64> {
        read_object(&self.path)
            .into_iter()
            .filter_map(|(uuid, value)| Some((uuid, epoch_value(&value)?)))
            .collect()
    }

    pub fn cooldown_until(&self, account_uuid: impl AsRef<str>, now: i64) -> Option<i64> {
        let uuid = account_uuid.as_ref().trim();
        if uuid.is_empty() {
            return None;
        }
        let until = self.cooldowns().get(uuid).copied()?;
        (until > now).then_some(until)
    }

    pub fn is_cooled(&self, account_uuid: impl AsRef<str>, now: i64) -> bool {
        self.cooldown_until(account_uuid, now).is_some()
    }

    pub fn set(&self, account_uuid: impl AsRef<str>, until: i64) -> Result<()> {
        let uuid = account_uuid.as_ref().trim().to_string();
        if uuid.is_empty() {
            return Ok(());
        }
        update_json_locked::<Value, _>(&self.path, &self.lock, |state| {
            object_mut(state).insert(uuid, Value::Number(Number::from(until)));
            Ok(())
        })?;
        Ok(())
    }

    pub fn set_cooldown(&self, account_uuid: impl AsRef<str>, until: i64) -> Result<()> {
        self.set(account_uuid, until)
    }

    pub fn clear(&self, account_uuid: impl AsRef<str>) -> Result<bool> {
        let uuid = account_uuid.as_ref().trim().to_string();
        if uuid.is_empty() {
            return Ok(false);
        }
        let mut removed = false;
        with_exclusive_lock(&self.lock, || {
            let mut object = match read_json::<Value>(&self.path) {
                Ok(Value::Object(object)) => object,
                _ => return Ok(()),
            };
            removed = object.remove(&uuid).is_some();
            if removed {
                write_json_atomic(&self.path, &Value::Object(object))?;
            }
            Ok(())
        })?;
        Ok(removed)
    }

    pub fn clear_cooldown(&self, account_uuid: impl AsRef<str>) -> Result<bool> {
        self.clear(account_uuid)
    }
}

fn read_object(path: &Path) -> Map<String, Value> {
    match read_json::<Value>(path) {
        Ok(Value::Object(object)) => object,
        _ => Map::new(),
    }
}

fn object_mut(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("object just initialized")
}

fn epoch_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| {
                number
                    .as_f64()
                    .filter(|value| value.is_finite())
                    .map(|value| value as i64)
            }),
        Value::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}
