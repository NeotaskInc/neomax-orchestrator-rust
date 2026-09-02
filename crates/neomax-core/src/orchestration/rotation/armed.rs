use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::StatePaths;
use crate::Result;
use crate::accounts;
use crate::atomic::{read_json, update_json_locked, with_exclusive_lock, write_json_atomic};

use super::selectors::normalize_profile_path;

pub const ARMED_ROTATE_AGE_SECONDS: i64 = 12 * 60 * 60;

fn default_threshold() -> f64 {
    accounts::LIVE_ROTATION_FIVE_PERCENT
}

fn default_weekly_threshold() -> f64 {
    accounts::LIVE_ROTATION_WEEKLY_PERCENT
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmedRotateRecord {
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    #[serde(default = "default_weekly_threshold")]
    pub weekly_threshold: f64,
    #[serde(default)]
    pub prefer: Vec<String>,
    #[serde(default)]
    pub auto: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default)]
    pub ts: i64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for ArmedRotateRecord {
    fn default() -> Self {
        Self {
            threshold: default_threshold(),
            weekly_threshold: default_weekly_threshold(),
            prefer: Vec::new(),
            auto: false,
            session: None,
            ts: 0,
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmedRotateClaim {
    pub threshold: f64,
    pub weekly_threshold: f64,
    #[serde(default)]
    pub prefer: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ArmedRotateStore {
    path: PathBuf,
    lock: PathBuf,
    max_age_seconds: i64,
}

impl ArmedRotateStore {
    pub fn new(path: impl Into<PathBuf>, lock: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: lock.into(),
            max_age_seconds: ARMED_ROTATE_AGE_SECONDS,
        }
    }

    pub fn in_state_dir(state: impl AsRef<Path>) -> Self {
        Self::in_paths(&StatePaths::new(
            PathBuf::new(),
            state.as_ref().to_path_buf(),
        ))
    }

    pub fn in_paths(paths: &StatePaths) -> Self {
        Self::new(paths.armed_rotate.clone(), paths.armed_rotate_lock.clone())
    }

    pub fn with_max_age_seconds(mut self, max_age_seconds: i64) -> Self {
        self.max_age_seconds = max_age_seconds;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn records(&self) -> BTreeMap<PathBuf, ArmedRotateRecord> {
        read_object(&self.path)
            .into_iter()
            .filter_map(|(key, value)| {
                if crate::io::is_rooted_but_not_absolute(Path::new(&key)) {
                    return None;
                }
                let record = serde_json::from_value(value).ok()?;
                Some((normalize_profile_path(key), record))
            })
            .collect()
    }

    pub fn get(&self, profile: impl AsRef<Path>) -> Option<ArmedRotateRecord> {
        let key = profile_key(profile.as_ref())?;
        read_object(&self.path)
            .into_iter()
            .find(|(candidate, _)| normalized_key(candidate).as_deref() == Some(key.as_str()))
            .and_then(|(_, value)| serde_json::from_value(value).ok())
    }

    pub fn arm(
        &self,
        profile: impl AsRef<Path>,
        threshold: f64,
        weekly_threshold: f64,
        prefer: &[String],
        auto: bool,
        now: i64,
    ) -> Result<ArmedRotateRecord> {
        let key = profile_key(profile.as_ref()).ok_or_else(|| invalid_profile(profile.as_ref()))?;
        let mut result = None;
        update_json_locked::<Value, _>(&self.path, &self.lock, |state| {
            let object = object_mut(state);
            let source_key = matching_key(object, &key);
            let existing = source_key
                .as_ref()
                .and_then(|source| object.get(source))
                .cloned()
                .and_then(|value| serde_json::from_value::<ArmedRotateRecord>(value).ok());
            if auto && existing.as_ref().is_some_and(|record| !record.auto) {
                if source_key.as_deref() != Some(key.as_str()) {
                    if let Some(source) = source_key {
                        if let Some(value) = object.remove(&source) {
                            object.insert(key.clone(), value);
                        }
                    }
                }
                result = existing;
                return Ok(());
            }
            let mut record = existing.unwrap_or_default();
            record.threshold = threshold;
            record.weekly_threshold = weekly_threshold;
            record.prefer = prefer.to_vec();
            record.auto = auto;
            record.ts = now;
            if source_key.as_deref() != Some(key.as_str()) {
                if let Some(source) = source_key {
                    object.remove(&source);
                }
            }
            object.insert(key.clone(), serde_json::to_value(&record)?);
            result = Some(record);
            Ok(())
        })?;
        Ok(result.unwrap_or_default())
    }

    pub fn clear(&self, profile: impl AsRef<Path>) -> Result<bool> {
        let key = profile_key(profile.as_ref()).ok_or_else(|| invalid_profile(profile.as_ref()))?;
        let mut removed = false;
        with_exclusive_lock(&self.lock, || {
            let mut object = match read_json::<Value>(&self.path) {
                Ok(Value::Object(object)) => object,
                _ => return Ok(()),
            };
            let keys = object
                .keys()
                .filter(|candidate| normalized_key(candidate).as_deref() == Some(key.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            removed = !keys.is_empty();
            for candidate in keys {
                object.remove(&candidate);
            }
            if removed {
                write_json_atomic(&self.path, &Value::Object(object))?;
            }
            Ok(())
        })?;
        Ok(removed)
    }

    pub fn claim(
        &self,
        profile: impl AsRef<Path>,
        session_id: Option<&str>,
        now: i64,
    ) -> Result<Option<ArmedRotateClaim>> {
        self.claim_or_refresh(profile.as_ref(), session_id, now)
    }

    pub fn refresh(
        &self,
        profile: impl AsRef<Path>,
        session_id: Option<&str>,
        now: i64,
    ) -> Result<Option<ArmedRotateClaim>> {
        self.claim_or_refresh(profile.as_ref(), session_id, now)
    }

    pub fn armed_rotate_take(
        &self,
        profile: impl AsRef<Path>,
        session_id: Option<&str>,
        now: i64,
    ) -> Result<Option<ArmedRotateClaim>> {
        self.claim(profile, session_id, now)
    }

    fn claim_or_refresh(
        &self,
        profile: &Path,
        session_id: Option<&str>,
        now: i64,
    ) -> Result<Option<ArmedRotateClaim>> {
        let key = profile_key(profile).ok_or_else(|| invalid_profile(profile))?;
        let requested_session = non_empty(session_id);
        let mut result = None;
        with_exclusive_lock(&self.lock, || {
            let mut object = match read_json::<Value>(&self.path) {
                Ok(Value::Object(object)) => object,
                _ => return Ok(()),
            };
            let Some(source_key) = matching_key(&object, &key) else {
                return Ok(());
            };
            let Some(value) = object.get(&source_key).cloned() else {
                return Ok(());
            };
            let Some(mut record) = serde_json::from_value::<ArmedRotateRecord>(value).ok() else {
                return Ok(());
            };
            if now.saturating_sub(record.ts) > self.max_age_seconds {
                return Ok(());
            }
            if record
                .session
                .as_deref()
                .zip(requested_session)
                .is_some_and(|(owner, requested)| owner != requested)
            {
                return Ok(());
            }
            if let Some(session) = requested_session {
                record.session = Some(session.to_string());
            }
            record.ts = now;
            result = Some(claim_from_record(&record));
            if source_key != key {
                object.remove(&source_key);
            }
            object.insert(key.clone(), serde_json::to_value(record)?);
            write_json_atomic(&self.path, &Value::Object(object))?;
            Ok(())
        })?;
        Ok(result)
    }
}

fn claim_from_record(record: &ArmedRotateRecord) -> ArmedRotateClaim {
    ArmedRotateClaim {
        threshold: record.threshold,
        weekly_threshold: record.weekly_threshold,
        prefer: (!record.prefer.is_empty()).then(|| record.prefer.clone()),
    }
}

fn profile_key(profile: &Path) -> Option<String> {
    if crate::io::is_rooted_but_not_absolute(profile) {
        return None;
    }
    Some(
        normalize_profile_path(profile)
            .to_string_lossy()
            .into_owned(),
    )
}

fn normalized_key(value: &str) -> Option<String> {
    profile_key(Path::new(value))
}

fn matching_key(object: &Map<String, Value>, key: &str) -> Option<String> {
    object
        .contains_key(key)
        .then(|| key.to_string())
        .or_else(|| {
            object
                .keys()
                .find(|candidate| normalized_key(candidate).as_deref() == Some(key))
                .cloned()
        })
}

fn invalid_profile(profile: &Path) -> crate::Error {
    crate::Error::InvalidArgument(format!(
        "profile path must not be rooted without an absolute prefix: {}",
        profile.display()
    ))
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
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
