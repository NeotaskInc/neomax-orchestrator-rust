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
}

impl ModelPrice {
    fn from_io(model: &str, input: f64, output: f64) -> Self {
        let openai = model.starts_with("gpt") || model.starts_with('o');
        Self {
            input,
            output,
            cache_write: if openai {
                0.0
            } else {
                round_four(input * CLAUDE_CACHE_WRITE_MULTIPLIER)
            },
            cache_read: round_four(input * CACHE_READ_MULTIPLIER),
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
        self.rates
            .get(&normalized)
            .copied()
            .or_else(|| {
                self.rates
                    .iter()
                    .find_map(|(name, price)| normalized.starts_with(name).then_some(*price))
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
}

const MODEL_IO: &[(&str, f64, f64)] = &[
    ("claude-opus-5", 5.0, 25.0),
    ("claude-opus-4-8", 5.0, 25.0),
    ("claude-opus-4-7", 5.0, 25.0),
    ("claude-fable-5", 10.0, 50.0),
    ("claude-sonnet-4-6", 3.0, 15.0),
    ("claude-haiku-4-5", 1.0, 5.0),
    ("gpt-5.6-sol", 5.0, 30.0),
    ("gpt-5.6-terra", 2.5, 15.0),
    ("gpt-5.6-luna", 0.5, 4.0),
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
    fn derives_cache_prices_and_normalizes_context_suffixes() {
        let prices = PriceCatalog::default();
        let claude = prices.price_for("claude-fable-5[1m]");
        assert_eq!(claude.cache_write, 12.5);
        assert_eq!(claude.cache_read, 1.0);

        let codex = prices.price_for("gpt-5.6-sol-fast");
        assert_eq!(codex.cache_write, 0.0);
        assert_eq!(codex.cache_read, 0.5);
    }

    #[test]
    fn estimates_raw_token_cost_at_per_million_rates() {
        let prices = PriceCatalog::default();
        assert_eq!(
            prices.estimate("gpt-5.6-sol", 1_000_000, 100_000, 50, 500_000),
            8.25
        );
    }
}
