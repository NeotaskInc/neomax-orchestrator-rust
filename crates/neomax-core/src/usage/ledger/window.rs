use std::collections::BTreeMap;

use crate::Engine;
use crate::usage::{LedgerKind, LedgerRecord};

pub(super) struct Window {
    cutoff: i64,
    now: i64,
    adds: BTreeMap<(Engine, String), LedgerRecord>,
    totals: BTreeMap<(Engine, String), Vec<LedgerRecord>>,
}

impl Window {
    pub(super) fn new(cutoff: i64, now: i64) -> Self {
        Self {
            cutoff,
            now,
            adds: BTreeMap::new(),
            totals: BTreeMap::new(),
        }
    }

    pub(super) fn push(&mut self, record: LedgerRecord) {
        if record.ts > self.now {
            return;
        }
        let key = (record.engine, record.id.clone());
        if record.kind == LedgerKind::Total {
            self.totals.entry(key).or_default().push(record);
        } else if (self.cutoff == 0 || record.ts >= self.cutoff)
            && self
                .adds
                .get(&key)
                .is_none_or(|previous| (record.output, super::parser_version(&record))
                    > (previous.output, super::parser_version(previous)))
        {
            self.adds.insert(key, record);
        }
    }

    pub(super) fn finish(self) -> Vec<LedgerRecord> {
        let mut records = self.adds.into_values().collect::<Vec<_>>();
        for mut snapshots in self.totals.into_values() {
            snapshots.sort_by_key(|record| {
                (
                    record.ts,
                    record.total_tokens(),
                    std::cmp::Reverse(super::parser_version(record)),
                    !record.extra.contains_key("last_token_usage"),
                )
            });
            let mut previous: Option<LedgerRecord> = None;
            for snapshot in snapshots {
                if previous
                    .as_ref()
                    .is_some_and(|prior| same_counters(&snapshot, prior))
                {
                    continue;
                }
                let delta = if previous
                    .as_ref()
                    .is_some_and(|prior| counters_decreased(&snapshot, prior))
                {
                    after_reset(&snapshot)
                } else {
                    difference(&snapshot, previous.as_ref())
                };
                previous = Some(snapshot);
                if self.cutoff == 0 || delta.ts >= self.cutoff {
                    records.push(delta);
                }
            }
        }
        records
    }
}

fn counters(record: &LedgerRecord) -> [u64; 5] {
    [
        record.input,
        record.output,
        record.reasoning,
        record.cache_write,
        record.cache_read,
    ]
}

fn same_counters(left: &LedgerRecord, right: &LedgerRecord) -> bool {
    counters(left) == counters(right)
}

fn counters_decreased(current: &LedgerRecord, previous: &LedgerRecord) -> bool {
    counters(current)
        .into_iter()
        .zip(counters(previous))
        .any(|(current, previous)| current < previous)
}

fn after_reset(current: &LedgerRecord) -> LedgerRecord {
    let mut delta = difference(current, None);
    if let Some(last) = current.extra.get("last_token_usage") {
        let values =
            ["in", "out", "cw", "cr"].map(|key| last.get(key).and_then(serde_json::Value::as_u64));
        if let [Some(input), Some(output), Some(write), Some(read)] = values {
            delta.input = input;
            delta.output = output;
            delta.cache_write = write;
            delta.cache_read = read;
            delta.reasoning = 0;
            delta.cost = None;
            delta.requests = Some(1);
            delta.completions = Some(1);
            return delta;
        }
    }
    // A restored counter may include older work. Do not bill that history again.
    delta.input = 0;
    delta.output = 0;
    delta.reasoning = 0;
    delta.cache_write = 0;
    delta.cache_read = 0;
    delta.cost = Some(0.0);
    delta.requests = Some(0);
    delta.completions = Some(0);
    delta.errors = 0;
    delta.rate_limits = 0;
    delta.extra.insert("usage_counter_gap".into(), true.into());
    delta
}

fn difference(current: &LedgerRecord, previous: Option<&LedgerRecord>) -> LedgerRecord {
    let mut delta = current.clone();
    delta.kind = LedgerKind::Add;
    delta.id = format!("{}:{}:{}", current.id, current.ts, current.total_tokens());
    delta.session = Some(
        current
            .session
            .clone()
            .unwrap_or_else(|| current.id.clone()),
    );
    if let Some(previous) = previous {
        delta.input = current.input.saturating_sub(previous.input);
        delta.output = current.output.saturating_sub(previous.output);
        delta.reasoning = current.reasoning.saturating_sub(previous.reasoning);
        delta.cache_write = current.cache_write.saturating_sub(previous.cache_write);
        delta.cache_read = current.cache_read.saturating_sub(previous.cache_read);
        delta.cost = current
            .cost
            .zip(previous.cost)
            .map(|(current, previous)| (current - previous).max(0.0));
        delta.requests = current
            .requests
            .map(|value| value.saturating_sub(previous.requests.unwrap_or(0)));
        delta.completions = current
            .completions
            .map(|value| value.saturating_sub(previous.completions.unwrap_or(0)));
        delta.errors = current.errors.saturating_sub(previous.errors);
        delta.rate_limits = current.rate_limits.saturating_sub(previous.rate_limits);
        if let Some(current_write) = current
            .extra
            .get("cache_write_1h")
            .and_then(serde_json::Value::as_u64)
        {
            let previous_write = previous
                .extra
                .get("cache_write_1h")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            delta.extra.insert(
                "cache_write_1h".into(),
                current_write.saturating_sub(previous_write).into(),
            );
        }
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{PriceCatalog, UsageLedger, build_usage_report};

    fn total(ts: i64, model: &str, input: u64, cached: u64, output: u64) -> LedgerRecord {
        serde_json::from_value(serde_json::json!({
            "ts":ts, "provider":"codex", "account":"fixture", "model":model,
            "id":"session-1", "kind":"total", "in":input, "cr":cached, "out":output
        }))
        .unwrap()
    }

    #[test]
    fn corrected_parser_metadata_replaces_legacy_model_attribution_without_adding_tokens() {
        let mut window = Window::new(0, 400);
        window.push(total(100, "gpt-6-astra", 1_000_000, 0, 1_000_000));
        let mut corrected = total(100, "gpt-5.6-luna", 1_000_000, 0, 1_000_000);
        corrected.extra.insert("usage_parser_version".into(), 2.into());
        window.push(corrected);
        let report = build_usage_report(&window.finish(), 30, 400, &PriceCatalog::default());
        assert_eq!(report.grand.output, 1_000_000);
        assert_eq!(report.grand.cost, 1.4);
        assert_eq!(report.by_model.len(), 1);
        assert_eq!(report.by_model[0].model, "gpt-5.6-luna");
    }

    #[test]
    fn window_subtracts_the_last_cumulative_baseline_before_its_cutoff() {
        let temp = tempfile::tempdir().unwrap();
        let ledger = UsageLedger::new(temp.path());
        ledger
            .append(&[
                total(100, "gpt-5.6-luna", 100, 500, 20),
                total(200, "gpt-5.6-luna", 150, 700, 40),
                total(300, "gpt-6-astra", 180, 900, 50),
            ])
            .unwrap();
        let records = ledger.read_windowed_since(150, 250).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            (records[0].input, records[0].cache_read, records[0].output),
            (50, 200, 20)
        );
        assert_eq!(records[0].session.as_deref(), Some("session-1"));
    }

    #[test]
    fn model_changes_price_only_the_usage_recorded_under_each_model() {
        let mut window = Window::new(0, 400);
        window.push(total(100, "gpt-5.6-luna", 1_000_000, 2_000_000, 1_000_000));
        window.push(total(200, "gpt-6-astra", 2_000_000, 3_000_000, 2_000_000));
        let report = build_usage_report(&window.finish(), 30, 400, &PriceCatalog::default());
        assert_eq!(
            (
                report.grand.input,
                report.grand.cache_read,
                report.grand.output
            ),
            (2_000_000, 3_000_000, 2_000_000)
        );
        assert_eq!(report.by_model.len(), 2);
        assert_eq!(report.grand.cost, 62.44);
        assert_eq!(report.by_session.len(), 1);
    }

    #[test]
    fn sorts_out_of_order_snapshots_and_ignores_duplicates_and_future_records() {
        let mut window = Window::new(0, 300);
        let first = total(100, "gpt-5.6-luna", 100, 500, 20);
        let second = total(200, "gpt-5.6-luna", 150, 700, 40);
        window.push(second.clone());
        window.push(first.clone());
        window.push(second);
        window.push(first);
        window.push(total(400, "gpt-6-astra", 9999, 9999, 9999));
        let records = window.finish();
        assert_eq!(records.len(), 2);
        assert_eq!(records.iter().map(|r| r.input).sum::<u64>(), 150);
        assert_eq!(records.iter().map(|r| r.output).sum::<u64>(), 40);
    }

    #[test]
    fn rolling_seven_and_thirty_days_are_not_month_to_date() {
        let now = 1_788_804_000;
        let temp = tempfile::tempdir().unwrap();
        let ledger = UsageLedger::new(temp.path());
        ledger
            .append(&[
                total(now - 20 * 86_400, "gpt-5.6-luna", 100, 500, 20),
                total(now - 3 * 86_400, "gpt-5.6-luna", 150, 700, 40),
            ])
            .unwrap();
        let seven = ledger.read_windowed(7, now).unwrap();
        let thirty = ledger.read_windowed(30, now).unwrap();
        assert_eq!(seven.iter().map(|r| r.output).sum::<u64>(), 20);
        assert_eq!(thirty.iter().map(|r| r.output).sum::<u64>(), 40);
    }

    #[test]
    fn additive_messages_keep_deduplication_and_provider_identity() {
        let mut window = Window::new(50, 300);
        let mut message = total(100, "gpt-5.6-luna", 100, 0, 20);
        message.kind = LedgerKind::Add;
        window.push(message.clone());
        window.push(message.clone());
        message.engine = Engine::Claude;
        window.push(message);
        assert_eq!(window.finish().len(), 2);
    }

    #[test]
    fn cumulative_explicit_cost_and_counts_are_subtracted_with_the_baseline() {
        let mut first = total(100, "gpt-6-astra", 100, 0, 20);
        first.cost = Some(2.0);
        first.requests = Some(4);
        first.completions = Some(3);
        let mut second = total(200, "gpt-6-astra", 200, 0, 40);
        second.cost = Some(5.0);
        second.requests = Some(7);
        second.completions = Some(6);
        let mut window = Window::new(150, 300);
        window.push(first);
        window.push(second);
        let rows = window.finish();
        assert_eq!(rows[0].cost, Some(3.0));
        assert_eq!(rows[0].requests, Some(3));
        assert_eq!(rows[0].completions, Some(3));
    }

    #[test]
    fn counter_reset_uses_last_request_and_then_continues_from_the_new_counter() {
        let mut window = Window::new(0, 400);
        window.push(total(100, "gpt-5.6-sol", 1000, 5000, 100));
        let mut reset = total(200, "gpt-6-astra", 300, 1000, 40);
        reset.extra.insert(
            "last_token_usage".into(),
            serde_json::json!({"in":20,"out":10,"cw":0,"cr":100}),
        );
        window.push(reset);
        window.push(total(300, "gpt-6-astra", 350, 1200, 60));
        let rows = window.finish();
        assert_eq!(rows.iter().map(|r| r.input).sum::<u64>(), 1070);
        assert_eq!(rows.iter().map(|r| r.output).sum::<u64>(), 130);
        assert_eq!(rows.iter().map(|r| r.cache_read).sum::<u64>(), 5300);
        assert!(
            rows.iter()
                .all(|r| !r.extra.contains_key("usage_counter_gap"))
        );
    }

    #[test]
    fn legacy_counter_reset_is_explicitly_incomplete_instead_of_double_billed() {
        let mut window = Window::new(0, 400);
        window.push(total(100, "gpt-5.6-sol", 1000, 5000, 100));
        window.push(total(200, "gpt-6-astra", 300, 1000, 40));
        window.push(total(300, "gpt-6-astra", 350, 1200, 60));
        let rows = window.finish();
        assert_eq!(rows.iter().map(|r| r.input).sum::<u64>(), 1050);
        assert_eq!(rows.iter().map(|r| r.output).sum::<u64>(), 120);
        assert_eq!(rows[1].extra["usage_counter_gap"], true);
        assert_eq!(rows[1].cost, Some(0.0));
    }

    #[test]
    fn recovered_request_metadata_supersedes_an_older_duplicate_without_rebilling() {
        let mut window = Window::new(0, 400);
        window.push(total(100, "gpt-5.6-sol", 1000, 5000, 100));
        let legacy = total(200, "gpt-6-astra", 300, 1000, 40);
        window.push(legacy.clone());
        let mut recovered = legacy;
        recovered.extra.insert(
            "last_token_usage".into(),
            serde_json::json!({"in":20,"out":10,"cw":0,"cr":100}),
        );
        window.push(recovered.clone());
        window.push(recovered);
        let rows = window.finish();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].input, 20);
        assert!(!rows[1].extra.contains_key("usage_counter_gap"));
    }
}
