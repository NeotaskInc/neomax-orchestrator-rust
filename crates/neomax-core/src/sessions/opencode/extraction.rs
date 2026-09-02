use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Result;

use super::super::artifacts::flatten_extra;
use super::super::types::SessionTokens;
use super::common::{epoch, is_rate_limit, model_string, tokens};
use super::schema::OpenCodeMessage;
use super::sqlite::read_messages;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenCodeUsageRecord {
    pub id: String,
    pub session_id: String,
    pub timestamp: i64,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub tokens: SessionTokens,
    #[serde(default)]
    pub cost: Option<f64>,
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

pub fn parse_message(message: &OpenCodeMessage) -> Option<OpenCodeUsageRecord> {
    if message.data.get("role").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let error = message
        .data
        .get("error")
        .is_some_and(|value| !value.is_null());
    let completed = message
        .data
        .get("time")
        .and_then(|time| time.get("completed"))
        .and_then(|value| epoch(Some(value)))
        .filter(|value| *value > 0);
    let model = message
        .data
        .get("model")
        .and_then(model_string)
        .or_else(|| {
            message
                .data
                .get("providerID")
                .and_then(Value::as_str)
                .zip(message.data.get("modelID").and_then(Value::as_str))
                .map(|(provider, model)| format!("{provider}/{model}"))
        })
        .or_else(|| {
            message
                .data
                .get("modelID")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let timestamp = completed.unwrap_or(message.created);
    let tokens = tokens(message.data.get("tokens"));
    let cost = message.data.get("cost").and_then(number);
    let rate_limited = error && is_rate_limit(message.data.get("error"));
    let mut extra = message
        .data
        .as_object()
        .map_or_else(BTreeMap::new, |object| {
            flatten_extra(
                object,
                &[
                    "role",
                    "tokens",
                    "time",
                    "error",
                    "model",
                    "providerID",
                    "modelID",
                    "agent",
                    "cost",
                ],
            )
        });
    if let Some(error) = message.data.get("error") {
        extra.insert("error".into(), error.clone());
    }
    Some(OpenCodeUsageRecord {
        id: message.id.clone(),
        session_id: message.session_id.clone(),
        timestamp,
        model,
        agent: message
            .data
            .get("agent")
            .and_then(Value::as_str)
            .map(str::to_owned),
        tokens,
        cost,
        requests: 1,
        completions: u64::from(completed.is_some() && !error),
        errors: u64::from(error),
        rate_limits: u64::from(rate_limited),
        extra,
    })
}

pub fn extract_usage(
    messages: impl IntoIterator<Item = OpenCodeMessage>,
) -> Vec<OpenCodeUsageRecord> {
    messages
        .into_iter()
        .filter_map(|message| parse_message(&message))
        .collect()
}

pub fn read_usage(db: &Path, cutoff: i64) -> Result<Vec<OpenCodeUsageRecord>> {
    Ok(extract_usage(read_messages(db, cutoff)?))
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|number| number.parse::<f64>().ok()))
        .filter(|number| number.is_finite() && *number >= 0.0)
}
