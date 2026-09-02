use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::super::artifacts::flatten_extra;
use super::super::headers::timestamp_epoch;
use super::super::types::SessionTokens;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrokUsageRecord {
    pub id: String,
    pub session_id: String,
    pub timestamp: i64,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tokens: SessionTokens,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub completions: u64,
    #[serde(default)]
    pub errors: u64,
    #[serde(default)]
    pub rate_limits: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

pub fn parse_usage_line(
    line: &str,
    session_id: &str,
    fallback_model: Option<&str>,
    fallback_ts: i64,
) -> Option<GrokUsageRecord> {
    let envelope = serde_json::from_str::<Value>(line).ok()?;
    let update = envelope
        .pointer("/params/update")
        .or_else(|| envelope.get("update"))?;
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("turn_completed") {
        return None;
    }
    let usage = update.get("usage")?.as_object()?;
    let input_full = number(usage, &["inputTokens", "input_tokens"]);
    let output = number(usage, &["outputTokens", "output_tokens"]);
    let cache_read = number(usage, &["cachedReadTokens", "cache_read_input_tokens"]);
    let cache_write = number(
        usage,
        &["cacheCreationTokens", "cache_creation_input_tokens"],
    );
    let reasoning = number(usage, &["reasoningTokens", "reasoning_tokens"]);
    let model_usage = usage.get("modelUsage").and_then(Value::as_object);
    let calls = number(usage, &["modelCalls"]);
    let calls = if calls == 0 {
        model_usage
            .into_iter()
            .flat_map(|models| models.values())
            .filter_map(|model| model.as_object())
            .map(|model| number(model, &["modelCalls"]))
            .fold(0, u64::saturating_add)
    } else {
        calls
    };
    if input_full == 0
        && output == 0
        && reasoning == 0
        && cache_read == 0
        && cache_write == 0
        && calls == 0
        && model_usage.is_none()
    {
        return None;
    }
    let model = model_usage
        .filter(|models| models.len() == 1)
        .and_then(|models| models.keys().next().cloned())
        .or_else(|| fallback_model.map(str::to_owned));
    let stop_reason = update
        .get("stop_reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let agent_result = update
        .get("agent_result")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let error_text = update.to_string().to_ascii_lowercase();
    let rate_limited = matches!(stop_reason.as_str(), "rate_limit" | "rate_limited")
        || rate_limit_text(agent_result)
        || rate_limit_text(&error_text);
    let failed = matches!(
        stop_reason.as_str(),
        "error" | "rate_limit" | "rate_limited" | "cancelled"
    );
    let cost = usage
        .get("total_cost_usd")
        .or_else(|| usage.get("costUsd"))
        .and_then(value_f64)
        .or_else(|| {
            usage
                .get("costUsdTicks")
                .or_else(|| usage.get("total_cost_usd_ticks"))
                .and_then(value_f64)
                .map(|ticks| ticks / 10_000_000_000.0)
        })
        .unwrap_or_default();
    let timestamp = envelope
        .get("timestamp")
        .or_else(|| update.get("timestamp"))
        .and_then(timestamp_epoch)
        .unwrap_or(fallback_ts);
    let id = update
        .get("prompt_id")
        .or_else(|| update.get("request_id"))
        .or_else(|| update.get("turn_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("turn-{}", digest(line)));
    let extra = update.as_object().map_or_else(BTreeMap::new, |object| {
        flatten_extra(
            object,
            &[
                "sessionUpdate",
                "usage",
                "stop_reason",
                "agent_result",
                "prompt_id",
                "request_id",
                "turn_id",
                "timestamp",
            ],
        )
    });
    Some(GrokUsageRecord {
        id,
        session_id: session_id.into(),
        timestamp,
        model,
        tokens: SessionTokens {
            input: input_full
                .saturating_sub(cache_read)
                .saturating_sub(cache_write),
            output,
            reasoning,
            cache_read,
            cache_write,
            cost,
            ..SessionTokens::default()
        },
        cost,
        requests: calls.max(1),
        completions: u64::from(!failed),
        errors: u64::from(failed),
        rate_limits: u64::from(rate_limited),
        extra,
    })
}

pub fn extract_usage(
    text: &str,
    session_id: &str,
    fallback_model: Option<&str>,
    fallback_ts: i64,
) -> Vec<GrokUsageRecord> {
    text.lines()
        .filter_map(|line| parse_usage_line(line, session_id, fallback_model, fallback_ts))
        .collect()
}

fn number(map: &serde_json::Map<String, Value>, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value_u64(map.get(*key)))
        .unwrap_or_default()
}

fn value_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
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
}

fn value_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|number| number.parse::<f64>().ok()))
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn rate_limit_text(value: &str) -> bool {
    let text = value.to_ascii_lowercase();
    text.contains("429") || text.contains("rate limit") || text.contains("too many requests")
}

fn digest(value: &str) -> String {
    let mut output = String::new();
    for byte in Sha256::digest(value.as_bytes()) {
        output.push_str(&format!("{byte:02x}"));
    }
    output[..20].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_call_aggregation_saturates() {
        let line = serde_json::json!({
            "params": {
                "update": {
                    "sessionUpdate": "turn_completed",
                    "usage": {
                        "modelUsage": {
                            "model-a": {"modelCalls": u64::MAX},
                            "model-b": {"modelCalls": 1}
                        }
                    }
                }
            }
        })
        .to_string();

        let record = parse_usage_line(&line, "session", None, 0).expect("usage record");
        assert_eq!(record.requests, u64::MAX);
    }
}
