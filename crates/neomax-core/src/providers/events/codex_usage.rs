use serde_json::Value;

use crate::providers::TokenUsage;

#[cfg(test)]
use super::common::json_lines;

#[derive(Debug, Default)]
pub(super) struct CodexUsageAccumulator {
    total: TokenUsage,
    cumulative: Option<TokenUsage>,
}

impl CodexUsageAccumulator {
    pub(super) fn observe(&mut self, event: &Value) {
        let payload = event.get("payload").unwrap_or(event);
        if payload.get("type").and_then(Value::as_str) == Some("token_count") {
            if let Some(info) = payload.get("info") {
                self.observe_wrapped(
                    info,
                    &["last_token_usage", "lastTokenUsage", "last"],
                    &["total_token_usage", "totalTokenUsage", "total"],
                );
            }
            return;
        }

        let params = event.get("params").or_else(|| payload.get("params"));
        let Some(usage) = event
            .get("usage")
            .or_else(|| payload.get("usage"))
            .or_else(|| event.get("tokenUsage"))
            .or_else(|| payload.get("tokenUsage"))
            .or_else(|| params.and_then(|params| params.get("tokenUsage")))
            .or_else(|| params.and_then(|params| params.get("token_usage")))
            .or_else(|| event.get("turn").and_then(|turn| turn.get("usage")))
            .or_else(|| {
                params.and_then(|params| params.get("turn").and_then(|turn| turn.get("usage")))
            })
            .or_else(|| params.and_then(|params| params.get("usage")))
            .or_else(|| event.get("token_usage"))
        else {
            return;
        };
        self.observe_wrapped(
            usage,
            &[
                "last_token_usage",
                "lastTokenUsage",
                "lastCallUsage",
                "lastCall",
                "last",
                "current",
            ],
            &["total_token_usage", "totalTokenUsage", "total"],
        );
    }

    pub(super) fn finish(self) -> TokenUsage {
        self.total
    }

    fn add_cumulative(&mut self, current: TokenUsage) {
        let delta = self.cumulative.as_ref().map_or_else(
            || current.clone(),
            |previous| cumulative_delta(previous, &current),
        );
        self.total.add(&delta);
        self.cumulative = Some(current);
    }

    fn observe_wrapped(&mut self, usage: &Value, last_keys: &[&str], total_keys: &[&str]) {
        let last = object_field(usage, last_keys);
        let total = object_field(usage, total_keys);
        match (last, total) {
            (Some(last), Some(total)) => {
                self.cumulative = Some(TokenUsage::from_codex_value(total));
                self.total.add(&TokenUsage::from_codex_value(last));
            }
            (Some(last), None) => self.total.add(&TokenUsage::from_codex_value(last)),
            (None, Some(total)) => self.add_cumulative(TokenUsage::from_codex_value(total)),
            (None, None) => self.total.add(&TokenUsage::from_codex_value(usage)),
        }
    }
}

fn object_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|key| value.get(*key).filter(|value| value.is_object()))
}

fn cumulative_delta(previous: &TokenUsage, current: &TokenUsage) -> TokenUsage {
    let reset = counter_reset(previous, current);
    TokenUsage {
        input: delta(previous.input, current.input, reset),
        output: delta(previous.output, current.output, reset),
        reasoning: delta(previous.reasoning, current.reasoning, reset),
        cache_read: delta(previous.cache_read, current.cache_read, reset),
        cache_write: delta(previous.cache_write, current.cache_write, reset),
        total: delta(previous.total, current.total, reset),
        cost: if reset {
            current.cost
        } else {
            (current.cost - previous.cost).max(0.0)
        },
        raw: current.raw.clone(),
    }
}

fn counter_reset(previous: &TokenUsage, current: &TokenUsage) -> bool {
    (previous.total > 0 && current.total > 0 && current.total < previous.total)
        || (current.input < previous.input && current.output < previous.output)
}

fn delta(previous: u64, current: u64, reset: bool) -> u64 {
    if reset {
        current
    } else {
        current.saturating_sub(previous)
    }
}

#[cfg(test)]
fn parse_stream_usage(bytes: &[u8]) -> TokenUsage {
    let mut accumulator = CodexUsageAccumulator::default();
    for event in json_lines(bytes) {
        accumulator.observe(&event);
    }
    accumulator.finish()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::common::stream;
    use super::*;

    #[test]
    fn sums_independent_turns_and_keeps_all_token_dimensions() {
        let bytes = stream(&[
            json!({"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":2,"reasoning_output_tokens":1,"cached_input_tokens":3,"cache_creation_input_tokens":4,"total_tokens":13}}),
            json!({"type":"turn.completed","usage":{"input_tokens":20,"output_tokens":5,"reasoning_output_tokens":2,"cached_input_tokens":6,"cache_creation_input_tokens":7,"total_tokens":25}}),
        ]);
        let usage = parse_stream_usage(&bytes);
        assert_eq!(usage.input, 21);
        assert_eq!(usage.output, 7);
        assert_eq!(usage.reasoning, 3);
        assert_eq!(usage.cache_read, 9);
        assert_eq!(usage.cache_write, 11);
        assert_eq!(usage.total, 38);
    }

    #[test]
    fn cumulative_snapshots_are_differenced_once() {
        let bytes = stream(&[
            json!({"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1,"total_tokens":13}}}),
            json!({"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1,"total_tokens":13}}}),
            json!({"type":"token_count","info":{"total_token_usage":{"input_tokens":30,"cached_input_tokens":5,"output_tokens":8,"reasoning_output_tokens":4,"total_tokens":38}}}),
        ]);
        let usage = parse_stream_usage(&bytes);
        assert_eq!(usage.input, 25);
        assert_eq!(usage.cache_read, 5);
        assert_eq!(usage.output, 8);
        assert_eq!(usage.reasoning, 4);
        assert_eq!(usage.total, 38);
    }

    #[test]
    fn last_usage_wins_over_cumulative_wrapper_without_double_counting() {
        let bytes = stream(&[json!({
            "type":"token_count",
            "info":{
                "total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"total_tokens":130},
                "last_token_usage":{"input_tokens":8,"cached_input_tokens":2,"output_tokens":3,"total_tokens":11}
            }
        })]);
        let usage = parse_stream_usage(&bytes);
        assert_eq!(usage.input, 6);
        assert_eq!(usage.cache_read, 2);
        assert_eq!(usage.output, 3);
        assert_eq!(usage.total, 11);
    }

    #[test]
    fn reads_app_server_total_and_last_usage_shapes() {
        let bytes = stream(&[
            json!({"type":"turn.completed","usage":{"total":{"totalTokens":100,"inputTokens":80,"cachedInputTokens":20,"outputTokens":20},"last":{"totalTokens":11,"inputTokens":8,"cachedInputTokens":2,"outputTokens":3}}}),
            json!({"type":"turn.completed","usage":{"total":{"totalTokens":120,"inputTokens":90,"cachedInputTokens":22,"outputTokens":30},"last":{"totalTokens":20,"inputTokens":10,"cachedInputTokens":2,"outputTokens":10}}}),
        ]);
        let usage = parse_stream_usage(&bytes);
        assert_eq!(usage.input, 14);
        assert_eq!(usage.cache_read, 4);
        assert_eq!(usage.output, 13);
        assert_eq!(usage.total, 31);
    }

    #[test]
    fn reads_app_server_token_usage_notifications() {
        let bytes = stream(&[
            json!({
                "method":"thread/tokenUsage/updated",
                "params":{"tokenUsage":{"last":{"inputTokens":8,"cachedInputTokens":2,"outputTokens":3,"totalTokens":11}}}
            }),
            json!({
                "method":"thread/tokenUsage/updated",
                "params":{"tokenUsage":{"last":{"inputTokens":10,"cachedInputTokens":2,"outputTokens":10,"totalTokens":20}}}
            }),
        ]);
        let usage = parse_stream_usage(&bytes);
        assert_eq!(usage.input, 14);
        assert_eq!(usage.cache_read, 4);
        assert_eq!(usage.output, 13);
        assert_eq!(usage.total, 31);
    }

    #[test]
    fn parses_checked_in_turn_and_cumulative_fixtures() {
        let turns = parse_stream_usage(include_bytes!(
            "../../../tests/fixtures/provider_events/codex-turns.jsonl"
        ));
        assert_eq!(turns.input, 21);
        assert_eq!(turns.output, 7);
        assert_eq!(turns.reasoning, 3);
        assert_eq!(turns.cache_read, 9);
        assert_eq!(turns.cache_write, 11);
        assert_eq!(turns.total, 38);

        let cumulative = parse_stream_usage(include_bytes!(
            "../../../tests/fixtures/provider_events/codex-cumulative.jsonl"
        ));
        assert_eq!(cumulative.input, 25);
        assert_eq!(cumulative.output, 8);
        assert_eq!(cumulative.reasoning, 4);
        assert_eq!(cumulative.cache_read, 5);
        assert_eq!(cumulative.total, 38);
    }
}
