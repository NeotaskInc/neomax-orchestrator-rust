use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::metrics::UsageMetrics;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalUsageEntry {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalToolUsageRow {
    pub tool: String,
    pub status: String,
    pub calls: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalErrorView {
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    pub message: String,
    #[serde(default)]
    pub at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalUsageTotals {
    #[serde(flatten)]
    pub metrics: UsageMetrics,
    #[serde(default)]
    pub sessions: u64,
    #[serde(default)]
    pub main_sessions: u64,
    #[serde(default)]
    pub native_subagents: u64,
    #[serde(default)]
    pub tool_calls: u64,
    #[serde(default)]
    pub tool_errors: u64,
    #[serde(default)]
    pub files: u64,
    #[serde(default)]
    pub adds: u64,
    #[serde(default)]
    pub dels: u64,
    #[serde(default)]
    pub last_activity: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalModelUsageRow {
    pub model: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalAgentUsageRow {
    pub agent: String,
    #[serde(flatten)]
    pub metrics: UsageMetrics,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsageDetail {
    pub available: bool,
    pub source: String,
    #[serde(default)]
    pub database: Option<PathBuf>,
    #[serde(default)]
    pub db_bytes: Option<u64>,
    pub account: String,
    pub window_days: u32,
    #[serde(default)]
    pub totals: LocalUsageTotals,
    #[serde(default)]
    pub models: Vec<LocalModelUsageRow>,
    #[serde(default)]
    pub agents: Vec<LocalAgentUsageRow>,
    #[serde(default)]
    pub tool_usage: Vec<LocalToolUsageRow>,
    #[serde(default)]
    pub last_error: Option<LocalErrorView>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LocalUsageSnapshot {
    pub available: bool,
    pub source: String,
    pub database: Option<PathBuf>,
    pub db_bytes: Option<u64>,
    pub account: String,
    pub window_days: u32,
    pub entries: Vec<LocalUsageEntry>,
    pub model_entries: Vec<LocalUsageEntry>,
    pub agent_entries: Vec<LocalUsageEntry>,
    pub sessions: u64,
    pub main_sessions: u64,
    pub native_subagents: u64,
    pub tool_usage: Vec<LocalToolUsageRow>,
    pub files: u64,
    pub adds: u64,
    pub dels: u64,
    pub tool_calls: Option<u64>,
    pub tool_errors: Option<u64>,
    pub last_activity: i64,
    pub last_error: Option<LocalErrorView>,
    pub error: Option<String>,
}

pub fn build_provider_usage_detail(snapshot: LocalUsageSnapshot) -> ProviderUsageDetail {
    let mut totals = LocalUsageTotals {
        sessions: snapshot.sessions,
        main_sessions: snapshot.main_sessions,
        native_subagents: snapshot.native_subagents,
        tool_calls: snapshot.tool_calls.unwrap_or_else(|| {
            snapshot
                .tool_usage
                .iter()
                .map(|row| row.calls)
                .fold(0, u64::saturating_add)
        }),
        tool_errors: snapshot.tool_errors.unwrap_or_else(|| {
            snapshot
                .tool_usage
                .iter()
                .filter(|row| row.status.eq_ignore_ascii_case("error"))
                .map(|row| row.calls)
                .fold(0, u64::saturating_add)
        }),
        files: snapshot.files,
        adds: snapshot.adds,
        dels: snapshot.dels,
        last_activity: snapshot.last_activity.max(0),
        ..LocalUsageTotals::default()
    };
    let mut by_model = BTreeMap::<String, UsageMetrics>::new();
    let mut by_agent = BTreeMap::<String, UsageMetrics>::new();
    for entry in snapshot.entries {
        totals.metrics.add(&entry.metrics);
        if let Some(model) = entry.model.as_deref() {
            by_model
                .entry(clean_text(model, 240))
                .or_default()
                .add(&entry.metrics);
        }
        if let Some(agent) = entry.agent.as_deref() {
            by_agent
                .entry(clean_text(agent, 160))
                .or_default()
                .add(&entry.metrics);
        }
    }
    for entry in snapshot.model_entries {
        if let Some(model) = entry.model.as_deref() {
            by_model
                .entry(clean_text(model, 240))
                .or_default()
                .add(&entry.metrics);
        }
    }
    for entry in snapshot.agent_entries {
        if let Some(agent) = entry.agent.as_deref() {
            by_agent
                .entry(clean_text(agent, 160))
                .or_default()
                .add(&entry.metrics);
        }
    }
    totals.metrics.round_cost();
    let mut models = by_model
        .into_iter()
        .map(|(model, mut metrics)| {
            metrics.round_cost();
            LocalModelUsageRow { model, metrics }
        })
        .collect::<Vec<_>>();
    let mut agents = by_agent
        .into_iter()
        .map(|(agent, mut metrics)| {
            metrics.round_cost();
            LocalAgentUsageRow { agent, metrics }
        })
        .collect::<Vec<_>>();
    let source = clean_text(&snapshot.source, 120);
    if source == "opencode.db" {
        models.sort_by(|left, right| {
            right
                .metrics
                .output
                .cmp(&left.metrics.output)
                .then_with(|| left.model.cmp(&right.model))
        });
    } else {
        models.sort_by(|left, right| left.model.cmp(&right.model));
    }
    agents.sort_by(|left, right| {
        right
            .metrics
            .output
            .cmp(&left.metrics.output)
            .then_with(|| left.agent.cmp(&right.agent))
    });
    let tool_usage = snapshot
        .tool_usage
        .into_iter()
        .map(|mut row| {
            row.tool = clean_text(&row.tool, 120);
            row.status = clean_text(&row.status, 80);
            row
        })
        .collect::<Vec<_>>();
    let mut tool_usage = tool_usage;
    tool_usage.sort_by(|left, right| {
        right
            .calls
            .cmp(&left.calls)
            .then_with(|| left.tool.cmp(&right.tool))
            .then_with(|| left.status.cmp(&right.status))
    });
    let last_error = snapshot.last_error.map(|mut error| {
        error.name = clean_text(&error.name, 80);
        error.status = error.status.map(|status| clean_text(&status, 40));
        error.message = clean_text(&error.message, 240);
        error.at = error.at.max(0);
        error
    });
    ProviderUsageDetail {
        available: snapshot.available,
        source,
        database: snapshot
            .database
            .map(|path| PathBuf::from(clean_text(&path.to_string_lossy(), 1_024))),
        db_bytes: snapshot.db_bytes,
        account: clean_text(&snapshot.account, 120),
        window_days: snapshot.window_days,
        totals,
        models,
        agents,
        tool_usage,
        last_error,
        error: snapshot.error.map(|error| clean_text(&error, 240)),
    }
}

fn clean_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::UsageCounts;

    fn entry(model: &str, agent: Option<&str>, output: u64, errors: u64) -> LocalUsageEntry {
        LocalUsageEntry {
            model: Some(model.into()),
            agent: agent.map(Into::into),
            metrics: UsageMetrics::from_counts(UsageCounts {
                input: 3,
                output,
                reasoning: 1,
                cache_read: 2,
                requests: 2,
                completions: 1,
                errors,
                ..Default::default()
            }),
        }
    }

    #[test]
    fn aggregates_local_models_agents_tools_and_unfinished_requests() {
        let detail = build_provider_usage_detail(LocalUsageSnapshot {
            available: true,
            source: "fixture".into(),
            account: "acct".into(),
            window_days: 7,
            entries: vec![
                entry("model-a", Some("main"), 4, 0),
                entry("model-a", Some("child"), 5, 1),
            ],
            sessions: 2,
            main_sessions: 1,
            native_subagents: 1,
            tool_usage: vec![LocalToolUsageRow {
                tool: "edit".into(),
                status: "error".into(),
                calls: 2,
            }],
            ..LocalUsageSnapshot::default()
        });
        assert_eq!(detail.totals.metrics.output, 9);
        assert_eq!(detail.totals.metrics.unfinished, 1);
        assert_eq!(detail.models.len(), 1);
        assert_eq!(detail.agents.len(), 2);
        assert_eq!(detail.totals.tool_calls, 2);
        assert_eq!(detail.totals.tool_errors, 2);
    }

    #[test]
    fn sanitizes_state_derived_detail_strings() {
        let detail = build_provider_usage_detail(LocalUsageSnapshot {
            source: "fixture\n<script>".into(),
            account: "acct\r\n".into(),
            entries: vec![entry("<model>", Some("<agent>"), 1, 0)],
            tool_usage: vec![LocalToolUsageRow {
                tool: "<tool>".into(),
                status: "ok\n".into(),
                calls: 1,
            }],
            error: Some("bad\n<error>".into()),
            ..LocalUsageSnapshot::default()
        });
        assert_eq!(detail.source, "fixture<script>");
        assert_eq!(detail.account, "acct");
        assert_eq!(detail.models[0].model, "<model>");
        assert_eq!(detail.tool_usage[0].status, "ok");
        assert_eq!(detail.error.as_deref(), Some("bad<error>"));
    }

    #[test]
    fn reference_kimi_detail_defaults_missing_metric_fields() {
        let detail: ProviderUsageDetail = serde_json::from_value(serde_json::json!({
            "available": true,
            "source": "kimi-local-wire",
            "account": "1",
            "window_days": 7,
            "totals": {
                "in": 3,
                "out": 4,
                "cw": 0,
                "cr": 0,
                "requests": 1,
                "completions": 1,
                "errors": 0,
                "rate_limits": 0,
                "sessions": 1,
                "main_sessions": 1,
                "native_subagents": 0,
                "tool_calls": 0,
                "tool_errors": 0,
                "files": 0
            },
            "models": [{"model": "kimi-code/k3", "completions": 1}]
        }))
        .unwrap();
        assert_eq!(detail.totals.metrics.output, 4);
        assert_eq!(detail.totals.metrics.cost, 0.0);
        assert_eq!(detail.models[0].metrics.completions, 1);
    }
}
