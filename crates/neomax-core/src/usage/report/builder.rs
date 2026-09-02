use std::cmp::Ordering;
use std::collections::BTreeMap;

use chrono::{Local, TimeZone};

use crate::Engine;

use super::metrics::{UsageCounts, UsageMetrics};
use super::rows::{
    AccountUsageRow, AgentUsageRow, DateUsageRow, ModelUsageRow, ProviderUsageRow, SessionUsageRow,
    UsageReport,
};
use crate::usage::pricing::PriceCatalog;
use crate::usage::types::LedgerRecord;

pub fn build_usage_report(
    records: &[LedgerRecord],
    days: u32,
    now: i64,
    prices: &PriceCatalog,
) -> UsageReport {
    let mut grand = UsageMetrics::default();
    let mut providers = BTreeMap::<Engine, UsageMetrics>::new();
    let mut accounts = BTreeMap::<(Engine, String), UsageMetrics>::new();
    let mut models = BTreeMap::<(Engine, String), UsageMetrics>::new();
    let mut dates = BTreeMap::<String, UsageMetrics>::new();
    let mut sessions = BTreeMap::<(Engine, String), UsageMetrics>::new();
    let mut agents = BTreeMap::<(Engine, String, String), UsageMetrics>::new();

    for record in records {
        let metrics = metrics_for(record, prices);
        grand.add(&metrics);
        providers.entry(record.engine).or_default().add(&metrics);
        accounts
            .entry((record.engine, record.account.clone()))
            .or_default()
            .add(&metrics);
        models
            .entry((record.engine, record.model.clone()))
            .or_default()
            .add(&metrics);
        dates
            .entry(local_date(record.ts))
            .or_default()
            .add(&metrics);
        sessions
            .entry((
                record.engine,
                record.session.clone().unwrap_or_else(|| record.id.clone()),
            ))
            .or_default()
            .add(&metrics);
        if let Some(agent) = &record.agent {
            agents
                .entry((record.engine, record.account.clone(), agent.clone()))
                .or_default()
                .add(&metrics);
        }
    }

    grand.round_cost();
    let mut report = UsageReport {
        days,
        now,
        grand,
        by_provider: providers
            .into_iter()
            .map(|(provider, metrics)| ProviderUsageRow { provider, metrics })
            .collect(),
        by_account: accounts
            .into_iter()
            .map(|((provider, account), metrics)| AccountUsageRow {
                provider,
                account,
                metrics,
            })
            .collect(),
        by_model: models
            .into_iter()
            .map(|((provider, model), metrics)| ModelUsageRow {
                provider,
                model,
                metrics,
            })
            .collect(),
        by_date: dates
            .into_iter()
            .map(|(date, metrics)| DateUsageRow { date, metrics })
            .collect(),
        by_session: sessions
            .into_iter()
            .map(|((provider, session), metrics)| SessionUsageRow {
                provider,
                session,
                metrics,
            })
            .collect(),
        by_agent: agents
            .into_iter()
            .map(|((provider, account, agent), metrics)| AgentUsageRow {
                provider,
                account,
                agent,
                metrics,
            })
            .collect(),
        opencode: Vec::new(),
        kimi: Vec::new(),
        grok: Vec::new(),
        pricing: prices.rates().clone(),
    };
    sort_report(&mut report);
    report
}

fn metrics_for(record: &LedgerRecord, prices: &PriceCatalog) -> UsageMetrics {
    let completions = record.completions.unwrap_or(1);
    let requests = record
        .requests
        .unwrap_or(if completions == 0 { 1 } else { completions });
    UsageMetrics::from_counts(UsageCounts {
        input: record.input,
        output: record.output,
        reasoning: record.reasoning,
        cache_write: record.cache_write,
        cache_read: record.cache_read,
        requests,
        completions,
        errors: record.errors,
        rate_limits: record.rate_limits,
        cost: record.cost.unwrap_or_else(|| {
            prices.estimate(
                &record.model,
                record.input,
                record.output,
                record.cache_write,
                record.cache_read,
            )
        }),
    })
}

fn local_date(timestamp: i64) -> String {
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or_else(Local::now)
        .format("%Y-%m-%d")
        .to_string()
}

fn rank(left: &UsageMetrics, right: &UsageMetrics) -> Ordering {
    right
        .cost
        .partial_cmp(&left.cost)
        .unwrap_or(Ordering::Equal)
        .then_with(|| right.ranked_tokens().cmp(&left.ranked_tokens()))
}

fn sort_report(report: &mut UsageReport) {
    for metrics in report_metrics_mut(report) {
        metrics.round_cost();
    }
    report
        .by_provider
        .sort_by(|left, right| rank(&left.metrics, &right.metrics));
    report
        .by_account
        .sort_by(|left, right| rank(&left.metrics, &right.metrics));
    report
        .by_model
        .sort_by(|left, right| rank(&left.metrics, &right.metrics));
    report
        .by_date
        .sort_by(|left, right| right.date.cmp(&left.date));
    report
        .by_session
        .sort_by(|left, right| rank(&left.metrics, &right.metrics));
    report.by_session.truncate(50);
    report
        .by_agent
        .sort_by(|left, right| rank(&left.metrics, &right.metrics));
}

fn report_metrics_mut(report: &mut UsageReport) -> impl Iterator<Item = &mut UsageMetrics> {
    report
        .by_provider
        .iter_mut()
        .map(|row| &mut row.metrics)
        .chain(report.by_account.iter_mut().map(|row| &mut row.metrics))
        .chain(report.by_model.iter_mut().map(|row| &mut row.metrics))
        .chain(report.by_date.iter_mut().map(|row| &mut row.metrics))
        .chain(report.by_session.iter_mut().map(|row| &mut row.metrics))
        .chain(report.by_agent.iter_mut().map(|row| &mut row.metrics))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::types::LedgerKind;

    fn record(engine: Engine, account: &str, model: &str, output: u64) -> LedgerRecord {
        LedgerRecord {
            ts: 1_800_000_000,
            engine,
            account: account.into(),
            model: model.into(),
            id: format!("{account}-{output}"),
            kind: LedgerKind::Add,
            session: Some("session".into()),
            agent: None,
            input: 1_000_000,
            output,
            reasoning: 0,
            cache_write: 0,
            cache_read: 0,
            cost: None,
            requests: None,
            completions: None,
            errors: 0,
            rate_limits: 0,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn builds_every_portal_group_and_preserves_zero_cost_providers() {
        let mut claude = record(Engine::Claude, "claude1", "claude-sonnet-4-6", 10);
        claude.agent = Some("agent-1".into());
        let kimi = record(Engine::Kimi, "kimi1", "kimi-code/k3", 20);
        let report =
            build_usage_report(&[claude, kimi], 30, 1_800_000_100, &PriceCatalog::default());

        assert_eq!(report.grand.input, 2_000_000);
        assert_eq!(report.by_provider.len(), 2);
        assert_eq!(report.by_account.len(), 2);
        assert_eq!(report.by_model.len(), 2);
        assert_eq!(report.by_date.len(), 1);
        assert_eq!(report.by_session.len(), 2);
        assert_eq!(report.by_agent.len(), 1);
        assert_eq!(report.by_provider[0].provider, Engine::Claude);
        assert_eq!(report.by_provider[1].metrics.cost, 0.0);
    }

    #[test]
    fn explicit_record_cost_and_request_counts_override_estimates() {
        let mut item = record(Engine::Opencode, "opencode1", "registry/model", 20);
        item.cost = Some(12.345);
        item.requests = Some(4);
        item.completions = Some(3);
        item.errors = 2;
        item.rate_limits = 1;
        let report = build_usage_report(&[item], 7, 20, &PriceCatalog::default());

        assert_eq!(report.grand.cost, 12.35);
        assert_eq!(report.grand.requests, 4);
        assert_eq!(report.grand.completions, 3);
        assert_eq!(report.grand.errors, 2);
        assert_eq!(report.grand.rate_limits, 1);
    }

    #[test]
    fn older_reports_deserialize_without_provider_local_detail_arrays() {
        let value = serde_json::json!({
            "days": 30,
            "now": 10,
            "grand": serde_json::to_value(UsageMetrics::default()).unwrap(),
            "by_provider": [],
            "by_account": [],
            "by_model": [],
            "by_date": [],
            "by_session": [],
            "by_agent": [],
            "pricing": {}
        });
        let report: UsageReport = serde_json::from_value(value).unwrap();
        assert!(report.opencode.is_empty());
        assert!(report.kimi.is_empty());
        assert!(report.grok.is_empty());
    }
}
