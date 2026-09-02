use serde_json::Value;

use crate::sessions::artifacts::json_lines;
use crate::sessions::types::SessionTokens;

pub fn claude_token_usage(text: &str) -> SessionTokens {
    let mut total = SessionTokens::default();
    for event in json_lines(text) {
        if event.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(usage) = event
            .get("message")
            .and_then(|message| message.get("usage"))
        else {
            continue;
        };
        let current = SessionTokens {
            input: integer(usage, &["input_tokens", "input"]),
            output: integer(usage, &["output_tokens", "output"]),
            reasoning: integer(usage, &["reasoning_tokens", "reasoning"]),
            cache_read: integer(usage, &["cache_read_input_tokens", "cache_read"]),
            cache_write: integer(usage, &["cache_creation_input_tokens", "cache_write"]),
            total: integer(usage, &["total_tokens", "total"]),
            cost: number(usage, &["cost", "cost_usd"]),
            ..SessionTokens::default()
        };
        total.add_assign(&current);
    }
    total
}

fn integer(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or_default()
}

fn number(value: &Value, keys: &[&str]) -> f64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_f64))
        .unwrap_or_default()
}
