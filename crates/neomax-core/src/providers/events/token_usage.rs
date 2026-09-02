use serde_json::{Map, Value};

use crate::providers::TokenUsage;

use super::json::{number_value, u64_value};

impl TokenUsage {
    pub fn from_value(value: &Value) -> Self {
        let Some(object) = value.as_object() else {
            return Self::default();
        };
        let input = number_alias(
            object,
            &["input", "input_tokens", "inputTokens", "inputOther"],
        );
        let output = number_alias(object, &["output", "output_tokens", "outputTokens"]);
        let reasoning = number_alias(
            object,
            &[
                "reasoning",
                "reasoning_tokens",
                "reasoningTokens",
                "reasoning_output_tokens",
                "reasoningOutputTokens",
                "thoughts",
                "thought_tokens",
            ],
        );
        let cache_read = number_alias(
            object,
            &[
                "cache_read",
                "cache_read_input_tokens",
                "cachedReadTokens",
                "cached_input_tokens",
                "cachedInputTokens",
                "cacheRead",
                "cached_tokens",
                "inputCacheRead",
            ],
        );
        let cache_write = number_alias(
            object,
            &[
                "cache_write",
                "cache_creation_input_tokens",
                "cacheCreationInputTokens",
                "cacheCreationTokens",
                "cacheWrite",
                "inputCacheCreation",
            ],
        );
        let cache = object.get("cache").and_then(Value::as_object);
        let cache_read = cache_read.max(
            cache
                .and_then(|item| item.get("read"))
                .and_then(u64_value)
                .unwrap_or(0),
        );
        let cache_write = cache_write.max(
            cache
                .and_then(|item| item.get("write"))
                .and_then(u64_value)
                .unwrap_or(0),
        );
        Self {
            input,
            output,
            reasoning,
            cache_read,
            cache_write,
            total: number_alias(object, &["total", "total_tokens", "totalTokens"]),
            cost: number_alias_f64(
                object,
                &[
                    "cost",
                    "cost_usd",
                    "costUSD",
                    "costUsd",
                    "total_cost_usd",
                    "totalCostUsd",
                ],
            ),
            raw: object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        }
    }

    pub(super) fn from_grok_value(value: &Value) -> Self {
        let mut usage = Self::from_value(value);
        if let Some(object) = value.as_object() {
            let prompt_input = ["promptTokens", "prompt_tokens"]
                .iter()
                .find_map(|key| object.get(*key).and_then(u64_value));
            if let Some(prompt_input) = prompt_input {
                usage.input = prompt_input.saturating_sub(usage.cache_read);
            }
        }
        usage
    }

    pub(super) fn from_codex_value(value: &Value) -> Self {
        let Some(object) = value.as_object() else {
            return Self::default();
        };
        let cache_read = number_alias(
            object,
            &[
                "cacheRead",
                "cache_read",
                "cache_read_input_tokens",
                "cachedReadTokens",
                "cached_input_tokens",
                "cachedInputTokens",
                "cached_tokens",
                "inputCacheRead",
            ],
        );
        let (input, input_is_total) = [
            "input_tokens",
            "inputTokens",
            "prompt_tokens",
            "promptTokens",
        ]
        .iter()
        .find_map(|key| {
            object
                .get(*key)
                .and_then(u64_value)
                .map(|value| (value, true))
        })
        .or_else(|| {
            ["input"].iter().find_map(|key| {
                object
                    .get(*key)
                    .and_then(u64_value)
                    .map(|value| (value, false))
            })
        })
        .unwrap_or((0, false));
        let mut usage = Self::from_value(value);
        usage.input = if input_is_total {
            input.saturating_sub(cache_read)
        } else {
            input
        };
        usage.cache_read = cache_read;
        usage.reasoning = number_alias(
            object,
            &[
                "reasoning_output_tokens",
                "reasoningOutputTokens",
                "reasoning_tokens",
                "reasoningTokens",
                "reasoning",
            ],
        );
        usage.cache_write = number_alias(
            object,
            &[
                "cache_write",
                "cacheWrite",
                "cacheCreationTokens",
                "cache_creation_input_tokens",
                "cacheCreationInputTokens",
                "cacheCreationTokens",
                "inputCacheCreation",
            ],
        );
        usage.total = number_alias(object, &["total_tokens", "totalTokens", "total"]);
        usage
    }

    pub fn add(&mut self, other: &Self) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.reasoning = self.reasoning.saturating_add(other.reasoning);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.total = self.total.saturating_add(other.total);
        self.cost += other.cost;
        self.raw.extend(other.raw.clone());
    }
}

fn number_alias(object: &Map<String, Value>, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(u64_value))
        .unwrap_or(0)
}

fn number_alias_f64(object: &Map<String, Value>, keys: &[&str]) -> f64 {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(number_value))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_usage_add_saturates_counters() {
        let mut usage = TokenUsage {
            input: u64::MAX,
            output: u64::MAX,
            reasoning: u64::MAX,
            cache_read: u64::MAX,
            cache_write: u64::MAX,
            total: u64::MAX,
            ..TokenUsage::default()
        };
        usage.add(&TokenUsage {
            input: 1,
            output: 1,
            reasoning: 1,
            cache_read: 1,
            cache_write: 1,
            total: 1,
            ..TokenUsage::default()
        });
        assert_eq!(usage.input, u64::MAX);
        assert_eq!(usage.output, u64::MAX);
        assert_eq!(usage.reasoning, u64::MAX);
        assert_eq!(usage.cache_read, u64::MAX);
        assert_eq!(usage.cache_write, u64::MAX);
        assert_eq!(usage.total, u64::MAX);
    }
}
