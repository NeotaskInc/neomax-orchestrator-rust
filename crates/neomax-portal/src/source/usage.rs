use anyhow::Result;

use neomax_core::config::Engine;
use neomax_core::usage::{
    AgentUsageRow, LedgerKind, LedgerRecord, PriceCatalog, ProviderUsageDetail, UsageLedger,
    UsageReport, build_usage_report,
};

use super::FilesystemPortalSource;

pub(crate) fn read_usage(
    source: &FilesystemPortalSource,
    days: u32,
    now: i64,
) -> Result<UsageReport> {
    let mut records =
        UsageLedger::new(source.paths.usage_ledger.clone()).read_deduplicated(days, now)?;
    let details = super::local_usage::read_details(source, days, now)?;
    for (engine, provider_details) in [
        (Engine::Opencode, &details[0]),
        (Engine::Kimi, &details[1]),
        (Engine::Grok, &details[2]),
    ] {
        for detail in provider_details.iter().filter(|detail| detail.available) {
            records.retain(|record| !(record.engine == engine && record.account == detail.account));
            records.extend(summary_records(engine, detail, now));
        }
    }
    let mut report = build_usage_report(&records, days, now, &PriceCatalog::default());
    report.opencode = details[0].clone();
    report.kimi = details[1].clone();
    report.grok = details[2].clone();
    append_local_agents(&mut report, Engine::Opencode, &details[0]);
    append_local_agents(&mut report, Engine::Kimi, &details[1]);
    append_local_agents(&mut report, Engine::Grok, &details[2]);
    Ok(report)
}

fn append_local_agents(report: &mut UsageReport, engine: Engine, details: &[ProviderUsageDetail]) {
    report.by_agent.extend(details.iter().flat_map(|detail| {
        detail.agents.iter().map(|agent| AgentUsageRow {
            provider: engine,
            account: detail.account.clone(),
            agent: agent.agent.clone(),
            metrics: agent.metrics.clone(),
        })
    }));
    report.by_agent.sort_by(|left, right| {
        right
            .metrics
            .cost
            .partial_cmp(&left.metrics.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .metrics
                    .input
                    .saturating_add(right.metrics.output)
                    .saturating_add(right.metrics.reasoning)
                    .cmp(
                        &left
                            .metrics
                            .input
                            .saturating_add(left.metrics.output)
                            .saturating_add(left.metrics.reasoning),
                    )
            })
    });
}

fn summary_records(engine: Engine, detail: &ProviderUsageDetail, now: i64) -> Vec<LedgerRecord> {
    if detail.models.is_empty() {
        if !has_usage(&detail.totals.metrics) {
            return Vec::new();
        }
        return vec![summary_record(
            engine,
            detail,
            now,
            None,
            &detail.totals.metrics,
        )];
    }
    detail
        .models
        .iter()
        .map(|row| summary_record(engine, detail, now, Some(&row.model), &row.metrics))
        .collect()
}

fn has_usage(metrics: &neomax_core::usage::UsageMetrics) -> bool {
    metrics.input > 0
        || metrics.output > 0
        || metrics.reasoning > 0
        || metrics.cache_write > 0
        || metrics.cache_read > 0
        || metrics.requests > 0
        || metrics.completions > 0
        || metrics.unfinished > 0
        || metrics.errors > 0
        || metrics.rate_limits > 0
        || metrics.cost != 0.0
}

fn summary_record(
    engine: Engine,
    detail: &ProviderUsageDetail,
    now: i64,
    model: Option<&String>,
    metrics: &neomax_core::usage::UsageMetrics,
) -> LedgerRecord {
    let ts = if detail.totals.last_activity > 0 {
        detail.totals.last_activity
    } else {
        now
    };
    let model = model
        .cloned()
        .unwrap_or_else(|| neomax_core::providers::catalog::default_model_id(engine).into());
    let id = format!(
        "local:{}:{}:{}:{}",
        engine.as_str(),
        detail.account,
        now,
        model
    );
    LedgerRecord {
        ts,
        engine,
        account: detail.account.clone(),
        model,
        id,
        kind: LedgerKind::Add,
        session: Some(format!("local:{}", detail.account)),
        agent: None,
        input: metrics.input,
        output: metrics.output,
        reasoning: metrics.reasoning,
        cache_write: metrics.cache_write,
        cache_read: metrics.cache_read,
        cost: Some(metrics.cost),
        requests: Some(metrics.requests),
        completions: Some(metrics.completions),
        errors: metrics.errors,
        rate_limits: metrics.rate_limits,
        extra: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FilesystemPortalSource;
    use neomax_core::usage::{LocalModelUsageRow, LocalUsageTotals, UsageCounts, UsageMetrics};

    #[test]
    fn empty_usage_ledger_still_returns_all_report_groups() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        let report = read_usage(&source, 30, 1_800_000_000).unwrap();
        assert_eq!(report.days, 30);
        assert_eq!(report.grand.input, 0);
        assert!(report.pricing.contains_key("claude-fable-5"));
    }

    #[test]
    fn local_summary_keeps_each_provider_model_in_generic_rows() {
        let detail = ProviderUsageDetail {
            available: true,
            source: "fixture".into(),
            account: "1".into(),
            window_days: 7,
            totals: LocalUsageTotals::default(),
            models: vec![
                LocalModelUsageRow {
                    model: "model-a".into(),
                    metrics: UsageMetrics::from_counts(UsageCounts {
                        input: 1,
                        output: 2,
                        requests: 1,
                        completions: 1,
                        ..UsageCounts::default()
                    }),
                },
                LocalModelUsageRow {
                    model: "model-b".into(),
                    metrics: UsageMetrics::from_counts(UsageCounts {
                        input: 3,
                        output: 4,
                        requests: 1,
                        completions: 1,
                        ..UsageCounts::default()
                    }),
                },
            ],
            ..ProviderUsageDetail::default()
        };
        let records = summary_records(Engine::Kimi, &detail, 1_800_000_000);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].model, "model-a");
        assert_eq!(records[1].model, "model-b");
    }
}
