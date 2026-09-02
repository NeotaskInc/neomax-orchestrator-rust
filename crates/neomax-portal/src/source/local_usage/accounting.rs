use std::collections::BTreeSet;

use neomax_core::sessions::SessionRecord;
use neomax_core::usage::{LocalUsageEntry, UsageCounts, UsageMetrics};

pub(crate) struct FileTotals {
    pub(crate) paths: BTreeSet<String>,
    pub(crate) adds: u64,
    pub(crate) dels: u64,
}

impl FileTotals {
    pub(crate) fn new() -> Self {
        Self {
            paths: BTreeSet::new(),
            adds: 0,
            dels: 0,
        }
    }

    pub(crate) fn include(&mut self, record: &SessionRecord) {
        self.paths
            .extend(record.files.iter().map(|file| file.path.clone()));
        self.adds = self.adds.saturating_add(
            record
                .files
                .iter()
                .map(|file| file.adds)
                .fold(0, u64::saturating_add),
        );
        self.dels = self.dels.saturating_add(
            record
                .files
                .iter()
                .map(|file| file.dels)
                .fold(0, u64::saturating_add),
        );
    }
}

pub(crate) fn entry(record: &SessionRecord) -> LocalUsageEntry {
    LocalUsageEntry {
        model: record.model.clone(),
        metrics: UsageMetrics::from_counts(UsageCounts {
            input: record.tokens.input,
            output: record.tokens.output,
            reasoning: record.tokens.reasoning,
            cache_write: record.tokens.cache_write,
            cache_read: record.tokens.cache_read,
            requests: record.requests,
            completions: record.completions,
            errors: record.errors,
            rate_limits: record.rate_limits,
            cost: record.tokens.cost,
        }),
        ..LocalUsageEntry::default()
    }
}

pub(crate) fn children_metrics(children: &[&SessionRecord], parent_id: &str) -> UsageMetrics {
    children
        .iter()
        .filter(|child| child.parent_id.as_deref() == Some(parent_id))
        .map(|child| entry(child).metrics)
        .fold(UsageMetrics::default(), |mut total, metrics| {
            super::metrics::add(&mut total, &metrics);
            total
        })
}
