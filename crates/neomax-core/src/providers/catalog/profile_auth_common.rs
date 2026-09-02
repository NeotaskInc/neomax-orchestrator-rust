use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::DateTime;
use serde_json::Value;

use super::filesystem::FileSystem;
use super::types::AuthMethod;

pub(super) fn unique_methods(methods: Vec<AuthMethod>) -> Vec<AuthMethod> {
    let mut unique = Vec::with_capacity(methods.len());
    for method in methods {
        if !unique.contains(&method) {
            unique.push(method);
        }
    }
    unique
}

pub(super) fn json_file(path: PathBuf, filesystem: &dyn FileSystem) -> Option<Value> {
    serde_json::from_slice(&filesystem.read(&path).ok()??).ok()
}

pub(super) fn read_utf8(path: PathBuf, filesystem: &dyn FileSystem) -> Option<String> {
    String::from_utf8(filesystem.read(&path).ok()??).ok()
}

pub(super) fn value_has_credential_key(value: &Value, keys: &[&str]) -> bool {
    value_has_credential_key_at(value, keys, epoch_now())
}

pub(super) fn value_has_credential_key_at(value: &Value, keys: &[&str], now: i64) -> bool {
    value
        .as_object()
        .is_some_and(|object| object_has_credential_at(object, keys, now))
}

pub(super) fn object_has_credential(
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> bool {
    object_has_credential_at(object, keys, epoch_now())
}

pub(super) fn object_has_credential_at(
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
    now: i64,
) -> bool {
    object_is_current(object, now) && object_has_any_credential(object, keys)
}

pub(super) fn credential_string(value: &Value) -> Option<&str> {
    value.as_str().filter(|value| !value.trim().is_empty())
}

fn object_has_any_credential(object: &serde_json::Map<String, Value>, keys: &[&str]) -> bool {
    keys.iter()
        .any(|key| object.get(*key).and_then(credential_string).is_some())
}

fn object_is_current(object: &serde_json::Map<String, Value>, now: i64) -> bool {
    EXPIRY_KEYS.iter().all(|key| {
        let Some(value) = object.get(*key) else {
            return true;
        };
        if value.is_null() {
            return true;
        }
        parse_epoch(value).is_some_and(|expires_at| expires_at > now)
    })
}

const EXPIRY_KEYS: &[&str] = &[
    "expiresAt",
    "expires_at",
    "expires",
    "expiration",
    "expirationDate",
    "expiry",
];

fn parse_epoch(value: &Value) -> Option<i64> {
    match value {
        Value::Number(value) => value.as_f64().and_then(normalize_epoch),
        Value::String(value) => {
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            value
                .parse::<f64>()
                .ok()
                .and_then(normalize_epoch)
                .or_else(|| parse_date(value))
        }
        _ => None,
    }
}

fn normalize_epoch(mut value: f64) -> Option<i64> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    while value > 100_000_000_000.0 {
        value /= 1000.0;
    }
    (value <= i64::MAX as f64).then_some(value as i64)
}

fn parse_date(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_rfc2822(value))
        .ok()
        .map(|date| date.timestamp())
}

fn epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}
