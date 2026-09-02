use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::providers::TokenUsage;
use crate::Engine;

use super::types::UsageRecord;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageAggregate {
    pub tokens: TokenUsage,
    pub requests: u64,
    pub errors: u64,
    pub cost: f64,
}

pub fn aggregate_by_engine(records: &[UsageRecord]) -> BTreeMap<Engine, UsageAggregate> {
    let mut output = BTreeMap::new();
    for record in records {
        let aggregate = output
            .entry(record.engine)
            .or_insert_with(UsageAggregate::default);
        aggregate.tokens.input = aggregate.tokens.input.saturating_add(record.tokens.input);
        aggregate.tokens.output = aggregate.tokens.output.saturating_add(record.tokens.output);
        aggregate.tokens.reasoning = aggregate
            .tokens
            .reasoning
            .saturating_add(record.tokens.reasoning);
        aggregate.tokens.cache_read = aggregate
            .tokens
            .cache_read
            .saturating_add(record.tokens.cache_read);
        aggregate.tokens.cache_write = aggregate
            .tokens
            .cache_write
            .saturating_add(record.tokens.cache_write);
        aggregate.tokens.total = aggregate.tokens.total.saturating_add(record.tokens.total);
        aggregate.requests = aggregate.requests.saturating_add(record.requests);
        aggregate.errors = aggregate.errors.saturating_add(record.errors);
        aggregate.cost += record.cost.unwrap_or(0.0);
    }
    output
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn aggregate_by_engine_saturates_counters() {
        let max = UsageRecord {
            timestamp: Utc::now(),
            engine: Engine::Claude,
            account: "account".into(),
            model: "model".into(),
            session_id: None,
            run_id: None,
            tokens: TokenUsage {
                input: u64::MAX,
                output: u64::MAX,
                reasoning: u64::MAX,
                cache_read: u64::MAX,
                cache_write: u64::MAX,
                total: u64::MAX,
                ..TokenUsage::default()
            },
            requests: u64::MAX,
            errors: u64::MAX,
            cost: None,
        };
        let one = UsageRecord {
            timestamp: max.timestamp,
            engine: max.engine,
            account: max.account.clone(),
            model: max.model.clone(),
            session_id: None,
            run_id: None,
            tokens: TokenUsage {
                input: 1,
                output: 1,
                reasoning: 1,
                cache_read: 1,
                cache_write: 1,
                total: 1,
                ..TokenUsage::default()
            },
            requests: 1,
            errors: 1,
            cost: None,
        };

        let aggregate = &aggregate_by_engine(&[max, one])[&Engine::Claude];
        assert_eq!(aggregate.tokens.input, u64::MAX);
        assert_eq!(aggregate.tokens.output, u64::MAX);
        assert_eq!(aggregate.tokens.reasoning, u64::MAX);
        assert_eq!(aggregate.tokens.cache_read, u64::MAX);
        assert_eq!(aggregate.tokens.cache_write, u64::MAX);
        assert_eq!(aggregate.tokens.total, u64::MAX);
        assert_eq!(aggregate.requests, u64::MAX);
        assert_eq!(aggregate.errors, u64::MAX);
    }
}
