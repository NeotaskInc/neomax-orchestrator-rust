use std::collections::BTreeMap;
use std::fmt::Write;

use chrono::DateTime;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::providers::catalog::{CLAUDE_DEFAULT_MODEL, CODEX_DEFAULT_MODEL, KIMI_DEFAULT_MODEL};
use crate::Engine;

use super::types::{LedgerKind, LedgerRecord};

pub fn parse_claude_line(line: &str, account: &str, fallback_ts: i64) -> Option<LedgerRecord> {
    if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
        return None;
    }
    let event: Value = serde_json::from_str(line).ok()?;
    let message = event.get("message")?.as_object()?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let usage = message.get("usage")?.as_object()?;
    if usage.is_empty() {
        return None;
    }
    let id = message
        .get("id")
        .or_else(|| event.get("requestId"))
        .or_else(|| event.get("uuid"))
        .and_then(Value::as_str)?;
    let model = message
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(CLAUDE_DEFAULT_MODEL);
    if model == "<synthetic>" {
        return None;
    }
    Some(LedgerRecord {
        ts: event_timestamp(&event, fallback_ts),
        engine: Engine::Claude,
        account: account.into(),
        model: model.into(),
        id: id.into(),
        kind: LedgerKind::Add,
        session: event
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string),
        agent: None,
        input: value_u64(usage.get("input_tokens")),
        output: value_u64(usage.get("output_tokens")),
        reasoning: 0,
        cache_write: value_u64(usage.get("cache_creation_input_tokens")),
        cache_read: value_u64(usage.get("cache_read_input_tokens")),
        cost: None,
        requests: None,
        completions: None,
        errors: 0,
        rate_limits: 0,
        extra: BTreeMap::new(),
    })
}

pub fn parse_codex_line(
    line: &str,
    account: &str,
    session_id: &str,
    model: Option<&str>,
    fallback_ts: i64,
) -> Option<LedgerRecord> {
    if !line.contains("\"token_count\"") || !line.contains("\"total_token_usage\"") {
        return None;
    }
    let event: Value = serde_json::from_str(line).ok()?;
    let payload = event.get("payload").unwrap_or(&event);
    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
        return None;
    }
    let usage = payload.get("info")?.get("total_token_usage")?.as_object()?;
    let total_input = value_u64(usage.get("input_tokens"));
    let cached = value_u64(usage.get("cached_input_tokens"));
    Some(LedgerRecord {
        ts: event_timestamp(&event, fallback_ts),
        engine: Engine::Codex,
        account: account.into(),
        model: model.unwrap_or(CODEX_DEFAULT_MODEL).into(),
        id: session_id.into(),
        kind: LedgerKind::Total,
        session: Some(session_id.into()),
        agent: None,
        input: total_input.saturating_sub(cached),
        output: value_u64(usage.get("output_tokens")),
        reasoning: 0,
        cache_write: 0,
        cache_read: cached,
        cost: None,
        requests: None,
        completions: None,
        errors: 0,
        rate_limits: 0,
        extra: BTreeMap::new(),
    })
}

pub fn parse_kimi_line(
    line: &str,
    account: &str,
    session_id: &str,
    agent_id: &str,
    fallback_ts: i64,
) -> Option<LedgerRecord> {
    if !line.contains("\"usage.record\"") {
        return None;
    }
    let event: Value = serde_json::from_str(line).ok()?;
    if event.get("type").and_then(Value::as_str) != Some("usage.record") {
        return None;
    }
    let usage = event.get("usage")?.as_object()?;
    Some(LedgerRecord {
        ts: event_timestamp(&event, fallback_ts),
        engine: Engine::Kimi,
        account: account.into(),
        model: event
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(KIMI_DEFAULT_MODEL)
            .into(),
        id: format!(
            "kimi:{session_id}:{agent_id}:{}",
            hash_prefix(line.as_bytes(), 20)
        ),
        kind: LedgerKind::Add,
        session: Some(session_id.into()),
        agent: Some(agent_id.into()),
        input: value_u64(usage.get("inputOther")),
        output: value_u64(usage.get("output")),
        reasoning: 0,
        cache_write: value_u64(usage.get("inputCacheCreation")),
        cache_read: value_u64(usage.get("inputCacheRead")),
        cost: Some(0.0),
        requests: None,
        completions: None,
        errors: 0,
        rate_limits: 0,
        extra: BTreeMap::new(),
    })
}

fn event_timestamp(event: &Value, fallback: i64) -> i64 {
    let value = event.get("timestamp").or_else(|| event.get("time"));
    match value {
        Some(Value::String(value)) => DateTime::parse_from_rfc3339(value)
            .map(|timestamp| timestamp.timestamp())
            .or_else(|_| value.parse::<f64>().map(normalize_epoch))
            .unwrap_or(fallback),
        Some(value) => value.as_f64().map(normalize_epoch).unwrap_or(fallback),
        None => fallback,
    }
}

fn normalize_epoch(mut value: f64) -> i64 {
    while value > 100_000_000_000.0 {
        value /= 1000.0;
    }
    value as i64
}

fn value_u64(value: Option<&Value>) -> u64 {
    value
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        })
        .unwrap_or(0)
}

fn hash_prefix(bytes: &[u8], length: usize) -> String {
    let hash = Sha256::digest(bytes);
    let mut output = String::with_capacity(hash.len() * 2);
    for byte in hash {
        let _ = write!(output, "{byte:02x}");
    }
    output.truncate(length.min(output.len()));
    output
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn claude_deduplicates_by_billed_message_id_not_transcript_uuid() {
        let line = json!({
            "timestamp":"2026-05-30T12:00:00Z",
            "sessionId":"session",
            "uuid":"content-block",
            "message":{
                "id":"message-billing-id",
                "role":"assistant",
                "model":"claude-fable-5",
                "usage":{"input_tokens":100,"output_tokens":20,"cache_creation_input_tokens":5,"cache_read_input_tokens":3}
            }
        })
        .to_string();
        let record = parse_claude_line(&line, "acct1", 0).unwrap();
        assert_eq!(record.id, "message-billing-id");
        assert_eq!(record.input, 100);
        assert_eq!(record.cache_read, 3);
    }

    #[test]
    fn codex_subtracts_cached_input_from_cumulative_input() {
        let line = json!({
            "timestamp":"2026-05-30T12:00:00Z",
            "payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":30,"output_tokens":20}}}
        })
        .to_string();
        let record = parse_codex_line(&line, "acct1", "session", Some("gpt-5.6-sol"), 0).unwrap();
        assert_eq!(record.input, 70);
        assert_eq!(record.cache_read, 30);
        assert_eq!(record.kind, LedgerKind::Total);
    }

    #[test]
    fn kimi_usage_ids_are_stable_per_wire_record() {
        let line = json!({
            "type":"usage.record",
            "time":1_800_000_000_000_i64,
            "model":"kimi-code/k3",
            "usage":{"inputOther":50,"output":20,"inputCacheCreation":2,"inputCacheRead":7}
        })
        .to_string();
        let left = parse_kimi_line(&line, "acct1", "session", "agent-0", 0).unwrap();
        let right = parse_kimi_line(&line, "acct1", "session", "agent-0", 0).unwrap();
        assert_eq!(left.id, right.id);
        assert_eq!(left.ts, 1_800_000_000);
    }
}
