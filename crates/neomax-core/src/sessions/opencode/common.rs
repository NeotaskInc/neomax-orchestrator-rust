use std::collections::BTreeMap;

use serde_json::Value;

use super::super::artifacts::flatten_extra;
use super::super::types::{FileActivity, SessionTokens};

pub(super) fn model_string(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Value>(value) {
            return model_string(&parsed);
        }
        return Some(value.to_string());
    }
    let object = value.as_object()?;
    let provider = object
        .get("providerID")
        .or_else(|| object.get("provider"))
        .and_then(Value::as_str);
    let model = object
        .get("id")
        .or_else(|| object.get("modelID"))
        .or_else(|| object.get("model"))
        .and_then(Value::as_str);
    match (provider, model) {
        (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
        (Some(provider), None) => Some(provider.into()),
        (None, Some(model)) => Some(model.into()),
        _ => None,
    }
}

pub(super) fn tokens(value: Option<&Value>) -> SessionTokens {
    let Some(value) = value else {
        return SessionTokens::default();
    };
    let cache = value.get("cache").unwrap_or(&Value::Null);
    SessionTokens {
        input: integer(value, &["input", "in", "input_tokens", "inputTokens"]),
        output: integer(value, &["output", "out", "output_tokens", "outputTokens"]),
        reasoning: integer(value, &["reasoning", "reasoning_tokens", "reasoningTokens"]),
        cache_read: integer(
            value,
            &[
                "cache_read",
                "cr",
                "cache_read_input_tokens",
                "cachedReadTokens",
            ],
        )
        .max(integer(cache, &["read", "cache_read", "cacheRead"])),
        cache_write: integer(
            value,
            &[
                "cache_write",
                "cw",
                "cache_creation_input_tokens",
                "cacheCreationTokens",
            ],
        )
        .max(integer(cache, &["write", "cache_write", "cacheWrite"])),
        total: integer(value, &["total", "total_tokens", "totalTokens"]),
        cost: value.get("cost").and_then(number).unwrap_or_default(),
        extra: value.as_object().map_or_else(BTreeMap::new, |object| {
            flatten_extra(
                object,
                &[
                    "input",
                    "in",
                    "input_tokens",
                    "inputTokens",
                    "output",
                    "out",
                    "output_tokens",
                    "outputTokens",
                    "reasoning",
                    "reasoning_tokens",
                    "reasoningTokens",
                    "cache_read",
                    "cr",
                    "cache_read_input_tokens",
                    "cachedReadTokens",
                    "cache_write",
                    "cw",
                    "cache_creation_input_tokens",
                    "cacheCreationTokens",
                    "cache",
                    "total",
                    "total_tokens",
                    "totalTokens",
                    "cost",
                ],
            )
        }),
    }
}

pub(super) fn files(value: Option<&Value>) -> Vec<FileActivity> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let path = file
                .get("path")
                .or_else(|| file.get("file_path"))
                .and_then(Value::as_str)?;
            Some(FileActivity {
                path: path.into(),
                adds: integer(file, &["adds", "additions"]),
                dels: integer(file, &["dels", "deletions"]),
                ops: integer(file, &["ops", "operations"]),
                extra: file.as_object().map_or_else(BTreeMap::new, |object| {
                    flatten_extra(
                        object,
                        &[
                            "path",
                            "file_path",
                            "adds",
                            "additions",
                            "dels",
                            "deletions",
                            "ops",
                            "operations",
                        ],
                    )
                }),
            })
        })
        .collect()
}

pub(super) fn integer(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| {
            value.get(*key).and_then(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
                    .or_else(|| {
                        value
                            .as_f64()
                            .filter(|number| number.is_finite() && *number >= 0.0)
                            .map(|number| number as u64)
                    })
                    .or_else(|| value.as_str().and_then(|number| number.parse::<u64>().ok()))
            })
        })
        .unwrap_or_default()
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|number| number.parse::<f64>().ok()))
        .filter(|number| number.is_finite() && *number >= 0.0)
}

pub(super) fn epoch(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    let number = value
        .as_i64()
        .or_else(|| value.as_f64().map(|n| n as i64))?;
    Some(normalize_epoch(number))
}

pub(super) fn normalize_epoch(mut number: i64) -> i64 {
    while number.unsigned_abs() > 100_000_000_000 {
        number /= 1000;
    }
    number
}

pub(super) fn line_count(value: Option<&Value>) -> u64 {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(|text| text.lines().count() as u64)
        .unwrap_or_default()
}

pub(super) fn is_rate_limit(value: Option<&Value>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let text = value.to_string().to_ascii_lowercase();
    text.contains("429") || text.contains("rate limit") || text.contains("too many requests")
}
