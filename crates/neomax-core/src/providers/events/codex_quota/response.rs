use serde_json::{Map, Value};

use super::types::CodexQuotaWindow;

pub(super) fn find_snapshot(value: &Value) -> Option<&Map<String, Value>> {
    let object = value.as_object()?;
    if object.contains_key("primary")
        || object.contains_key("secondary")
        || object.contains_key("rateLimitReachedType")
        || object.contains_key("rate_limit_reached_type")
    {
        return Some(object);
    }
    for key in [
        "rateLimits",
        "rate_limits",
        "payload",
        "data",
        "items",
        "rateLimitsByLimitId",
        "rate_limits_by_limit_id",
    ] {
        if let Some(child) = object.get(key) {
            if let Some(snapshot) = find_snapshot(child) {
                return Some(snapshot);
            }
            if matches!(key, "rateLimitsByLimitId" | "rate_limits_by_limit_id") {
                if let Some(entries) = child.as_object() {
                    let preferred = entries
                        .get("codex")
                        .or_else(|| entries.get("CODEX"))
                        .into_iter()
                        .chain(entries.iter().filter_map(|(key, value)| {
                            (!matches!(key.as_str(), "codex" | "CODEX")).then_some(value)
                        }));
                    for entry in preferred {
                        if let Some(snapshot) = find_snapshot(entry) {
                            return Some(snapshot);
                        }
                    }
                }
            }
            if let Some(items) = child.as_array() {
                for item in items {
                    if let Some(snapshot) = find_snapshot(item) {
                        return Some(snapshot);
                    }
                }
            }
        }
    }
    None
}

pub(super) fn parse_window(value: Option<&Value>) -> Option<CodexQuotaWindow> {
    let object = value?.as_object()?;
    let used_percent = number(object, &["usedPercent", "used_percent", "percent", "used"]);
    let window_minutes = number(
        object,
        &["windowDurationMins", "window_minutes", "windowMinutes"],
    )
    .map(|value| value as u64);
    let resets_at = object
        .get("resetsAt")
        .or_else(|| object.get("resets_at"))
        .and_then(epoch);
    (used_percent.is_some() || window_minutes.is_some() || resets_at.is_some()).then_some(
        CodexQuotaWindow {
            used_percent,
            window_minutes,
            resets_at,
        },
    )
}

fn number(object: &Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        object.get(*key).and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_u64().map(|value| value as f64))
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        })
    })
}

fn epoch(value: &Value) -> Option<f64> {
    if let Some(number) = value
        .as_f64()
        .or_else(|| value.as_u64().map(|value| value as f64))
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    {
        let mut normalized = number;
        while normalized > 100_000_000_000.0 {
            normalized /= 1000.0;
        }
        return (normalized > 0.0).then_some(normalized);
    }
    value
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp_millis() as f64 / 1000.0)
}
