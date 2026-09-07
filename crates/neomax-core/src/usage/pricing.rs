use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

const CACHE_READ_MULTIPLIER: f64 = 0.1;
const CLAUDE_CACHE_WRITE_MULTIPLIER: f64 = 1.25;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelPrice {
    #[serde(rename = "in")]
    pub input: f64,
    #[serde(rename = "out")]
    pub output: f64,
    #[serde(rename = "cw")]
    pub cache_write: f64,
    #[serde(rename = "cr")]
    pub cache_read: f64,
    #[serde(default, rename = "cw1h", skip_serializing_if = "Option::is_none")]
    pub cache_write_1h: Option<f64>,
}

impl ModelPrice {
    fn from_io(model: &str, input: f64, output: f64) -> Self {
        let openai = model.starts_with("gpt") || model.starts_with('o');
        Self {
            input,
            output,
            cache_write: if openai && !model.starts_with("gpt-5.6-") && model != "gpt-6-astra" {
                0.0
            } else {
                round_four(input * CLAUDE_CACHE_WRITE_MULTIPLIER)
            },
            cache_read: round_four(input * if model == "claude-fable-5-1" { 0.025 } else { CACHE_READ_MULTIPLIER }),
            cache_write_1h: model.starts_with("claude-").then(|| round_four(input * 2.0)),
        }
    }

    pub fn estimate(self, input: u64, output: u64, cache_write: u64, cache_read: u64) -> f64 {
        (input as f64 * self.input
            + output as f64 * self.output
            + cache_write as f64 * self.cache_write
            + cache_read as f64 * self.cache_read)
            / 1_000_000.0
    }
}

#[derive(Debug, Clone)]
pub struct PriceCatalog {
    rates: BTreeMap<String, ModelPrice>,
    fallback: ModelPrice,
}

impl Default for PriceCatalog {
    fn default() -> Self {
        let mut rates = BTreeMap::new();
        for (model, input, output) in MODEL_IO {
            rates.insert(
                (*model).to_string(),
                ModelPrice::from_io(model, *input, *output),
            );
        }
        Self {
            rates,
            fallback: ModelPrice::from_io("claude-fable-5", 10.0, 50.0),
        }
    }
}

impl PriceCatalog {
    pub fn rates(&self) -> &BTreeMap<String, ModelPrice> {
        &self.rates
    }

    pub fn price_for(&self, model: &str) -> ModelPrice {
        let normalized = model
            .to_ascii_lowercase()
            .replace("[1m]", "")
            .trim()
            .to_string();
        let normalized = normalized.strip_prefix("anthropic/").or_else(|| normalized.strip_prefix("openai/"))
            .unwrap_or(&normalized).replace("claude-fable-5.1", "claude-fable-5-1");
        self.rates
            .get(&normalized)
            .copied()
            .or_else(|| {
                self.rates
                    .iter()
                    .filter(|(name, _)| normalized.starts_with(name.as_str()))
                    .max_by_key(|(name, _)| name.len())
                    .map(|(_, price)| *price)
            })
            .unwrap_or(self.fallback)
    }

    pub fn estimate(
        &self,
        model: &str,
        input: u64,
        output: u64,
        cache_write: u64,
        cache_read: u64,
    ) -> f64 {
        self.price_for(model)
            .estimate(input, output, cache_write, cache_read)
    }

    pub fn estimate_record(&self, record: &super::LedgerRecord) -> f64 {
        let price = self.price_for(&record.model);
        let one_hour = record.extra.get("cache_write_1h").and_then(serde_json::Value::as_u64)
            .unwrap_or(0).min(record.cache_write);
        let mut cost = price.estimate(record.input, record.output, record.cache_write, record.cache_read);
        if let Some(rate) = price.cache_write_1h {
            cost += one_hour as f64 * (rate - price.cache_write) / 1_000_000.0;
        }
        cost
    }
}

const MODEL_IO: &[(&str, f64, f64)] = &[
    ("claude-opus-5", 5.0, 25.0),
    ("claude-opus-4-8", 5.0, 25.0),
    ("claude-opus-4-7", 5.0, 25.0),
    ("claude-fable-5", 10.0, 50.0),
    ("claude-fable-5-1", 10.0, 50.0),
    ("claude-sonnet-5", 2.0, 10.0),
    ("claude-sonnet-4-6", 3.0, 15.0),
    ("claude-haiku-4-5", 1.0, 5.0),
    ("gpt-6-astra", 10.0, 50.0),
    ("gpt-5.6-sol", 4.0, 20.0),
    ("gpt-5.6-terra", 2.0, 12.0),
    ("gpt-5.6-luna", 0.2, 1.2),
    ("gpt-5.5", 5.0, 30.0),
    ("gpt-5.4", 2.5, 15.0),
    ("kimi-code/k3", 0.0, 0.0),
    ("kimi-code/kimi-for-coding", 0.0, 0.0),
    ("grok-4.6", 0.0, 0.0),
];

fn round_four(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_models_have_distinct_cache_prices_and_longest_prefix_matching() {
        let prices = PriceCatalog::default();
        for (model, input, output, read, write) in [
            ("gpt-6-astra", 10.0, 50.0, 1.0, 12.5),
            ("gpt-5.6-sol", 4.0, 20.0, 0.4, 5.0),
            ("gpt-5.6-terra", 2.0, 12.0, 0.2, 2.5),
            ("gpt-5.6-luna", 0.2, 1.2, 0.02, 0.25),
            ("claude-fable-5", 10.0, 50.0, 1.0, 12.5),
            ("claude-fable-5-1", 10.0, 50.0, 0.25, 12.5),
        ] {
            let price = prices.price_for(model);
            assert_eq!((price.input, price.output, price.cache_read, price.cache_write), (input, output, read, write), "{model}");
        }
        for model in ["claude-fable-5-1[1m]", "claude-fable-5-1-20260901", "anthropic/claude-fable-5.1"] {
            assert_eq!(prices.price_for(model).cache_read, 0.25, "{model}");
        }
        let mut record: super::super::LedgerRecord = serde_json::from_value(serde_json::json!({
            "ts":1,"provider":"claude","account":"1","model":"claude-fable-5-1","id":"fixture","kind":"add",
            "in":1_000_000,"out":1_000_000,"cw":1_000_000,"cr":1_000_000,"cache_write_1h":400_000
        })).unwrap();
        assert_eq!(prices.estimate_record(&record), 75.75);
        record.model = "claude-fable-5".into();
        assert_eq!(prices.estimate_record(&record), 76.5);
    }

    #[test]
    fn derives_cache_prices_and_normalizes_context_suffixes() {
        let prices = PriceCatalog::default();
        let claude = prices.price_for("claude-fable-5[1m]");
        assert_eq!(claude.cache_write, 12.5);
        assert_eq!(claude.cache_read, 1.0);

        let codex = prices.price_for("gpt-5.6-sol-fast");
        assert_eq!(codex.cache_write, 5.0);
        assert_eq!(codex.cache_read, 0.4);
    }

    #[test]
    fn estimates_raw_token_cost_at_per_million_rates() {
        let prices = PriceCatalog::default();
        assert_eq!(
            prices.estimate("gpt-5.6-sol", 1_000_000, 100_000, 50, 500_000),
            6.20025
        );
    }
}
