use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UsageMetrics {
    #[serde(rename = "in")]
    pub input: u64,
    #[serde(rename = "out")]
    pub output: u64,
    pub reasoning: u64,
    #[serde(rename = "cw")]
    pub cache_write: u64,
    #[serde(rename = "cr")]
    pub cache_read: u64,
    pub requests: u64,
    pub completions: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unfinished: u64,
    pub errors: u64,
    pub rate_limits: u64,
    pub cost: f64,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsageCounts {
    pub input: u64,
    pub output: u64,
    pub reasoning: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    pub requests: u64,
    pub completions: u64,
    pub errors: u64,
    pub rate_limits: u64,
    pub cost: f64,
}

impl UsageMetrics {
    pub(crate) fn add(&mut self, other: &Self) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.reasoning = self.reasoning.saturating_add(other.reasoning);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.requests = self.requests.saturating_add(other.requests);
        self.completions = self.completions.saturating_add(other.completions);
        self.unfinished = self.unfinished.saturating_add(other.unfinished);
        self.errors = self.errors.saturating_add(other.errors);
        self.rate_limits = self.rate_limits.saturating_add(other.rate_limits);
        self.cost += other.cost;
    }

    pub(super) fn ranked_tokens(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.reasoning)
    }

    pub(crate) fn round_cost(&mut self) {
        self.cost = (self.cost * 100.0).round() / 100.0;
    }

    pub fn from_counts(counts: UsageCounts) -> Self {
        Self {
            input: counts.input,
            output: counts.output,
            reasoning: counts.reasoning,
            cache_write: counts.cache_write,
            cache_read: counts.cache_read,
            requests: counts.requests,
            completions: counts.completions,
            unfinished: counts
                .requests
                .saturating_sub(counts.completions.saturating_add(counts.errors)),
            errors: counts.errors,
            rate_limits: counts.rate_limits,
            cost: counts.cost,
        }
    }
}
