use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;
use neomax_core::orchestration::auth::{RotationEvent, RotationLog};
use serde_json::{Value, json};

use crate::model::{EngineView, RunView, SummaryView};
use crate::source::FilesystemPortalSource;

const ROTATION_WINDOW_SECONDS: i64 = 6 * 60 * 60;
const ROTATION_DISPLAY_LIMIT: usize = 6;
const ROTATION_LOG_READ_LIMIT: usize = 10_000;

pub(crate) fn build_summary(
    engines: &BTreeMap<String, EngineView>,
    runs: &[RunView],
    tasks: &[Value],
    inbox: usize,
    now: i64,
) -> SummaryView {
    let mut live_total = 0;
    let mut workers = 0;
    let mut mains = 0;
    let mut subagents = 0;
    let mut accounts_up = 0;
    let mut accounts_total = 0;
    let mut cooling = 0;
    let mut rotate_advised = Vec::new();
    let mut weekly = BTreeMap::<String, Option<f64>>::new();
    let mut orch_accounts = BTreeMap::new();
    let mut weekly_telemetry = BTreeMap::<String, Value>::new();
    let mut quota_capabilities = BTreeSet::new();
    let mut quota_available = BTreeSet::new();
    let mut quota_reactive = BTreeSet::new();
    for (engine, view) in engines {
        if view.capabilities.quota.supported {
            quota_capabilities.insert(engine.clone());
        }
        if view.capabilities.quota.reactive {
            quota_reactive.insert(engine.clone());
        }
        if view.capabilities.quota.available {
            quota_available.insert(engine.clone());
        }
        let mut min_weekly = None;
        let mut telemetry = BTreeMap::<String, u64>::new();
        for account in &view.accounts {
            live_total += account.live;
            workers += account.workers;
            mains += account.mains;
            subagents += account.subagents;
            accounts_total += 1;
            accounts_up += usize::from(account.authenticated);
            cooling += usize::from(account.cooldown_until > now);
            if account.rotate_advised {
                rotate_advised.push(json!({
                    "engine": engine,
                    "n": account.n,
                    "live": account.live,
                    "agents": account.agents,
                }));
            }
            if account.role == "orchestrator" {
                orch_accounts.insert(engine.clone(), true);
            }
            if account.capabilities.quota.supported {
                quota_capabilities.insert(engine.clone());
            }
            if account.capabilities.quota.reactive {
                quota_reactive.insert(engine.clone());
            }
            if account.capabilities.quota.available {
                quota_available.insert(engine.clone());
            }
            let current = account
                .usage
                .as_ref()
                .and_then(|value| value.get("seven_day"))
                .and_then(|value| value.get("used_percent"))
                .and_then(number);
            if let Some(value) = current {
                min_weekly = Some(min_weekly.map_or(value, |previous: f64| previous.min(value)));
            }
            if let Some(values) = account
                .telemetry
                .as_ref()
                .and_then(|value| value.get("totals"))
                .and_then(Value::as_object)
            {
                for (key, value) in values {
                    if let Some(value) = value.as_u64() {
                        let entry = telemetry.entry(key.clone()).or_default();
                        *entry = entry.saturating_add(value);
                    }
                }
            }
        }
        weekly.insert(engine.clone(), min_weekly);
        weekly_telemetry.insert(engine.clone(), json!(telemetry));
    }
    let open_tasks = tasks
        .iter()
        .filter(|task| {
            !task
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| {
                    matches!(status, "done" | "completed" | "closed" | "cancelled")
                })
        })
        .count();
    let running = runs
        .iter()
        .filter(|run| matches!(run.status.as_str(), "running" | "orphaned"))
        .count();
    SummaryView {
        live_total,
        running,
        workers,
        mains,
        subagents,
        agents_total: workers.saturating_add(mains).saturating_add(subagents),
        accounts_up,
        accounts_total,
        cooling,
        inbox,
        tasks_open: open_tasks,
        runs_total: runs.len(),
        claude_7d: weekly_telemetry
            .remove("claude")
            .unwrap_or_else(|| json!({})),
        codex_7d: weekly_telemetry
            .remove("codex")
            .unwrap_or_else(|| json!({})),
        opencode_7d: weekly_telemetry
            .remove("opencode")
            .unwrap_or_else(|| json!({})),
        kimi_7d: weekly_telemetry.remove("kimi").unwrap_or_else(|| json!({})),
        grok_7d: weekly_telemetry.remove("grok").unwrap_or_else(|| json!({})),
        claude_weekly_min: weekly.get("claude").copied().flatten(),
        codex_weekly_min: weekly.get("codex").copied().flatten(),
        opencode_weekly_min: weekly.get("opencode").copied().flatten(),
        kimi_weekly_min: weekly.get("kimi").copied().flatten(),
        grok_weekly_min: weekly.get("grok").copied().flatten(),
        claude_weekly_soft: weekly_soft(weekly.get("claude").copied().flatten()),
        codex_weekly_soft: weekly_soft(weekly.get("codex").copied().flatten()),
        opencode_weekly_soft: weekly_soft(weekly.get("opencode").copied().flatten()),
        kimi_weekly_soft: weekly_soft(weekly.get("kimi").copied().flatten()),
        grok_weekly_soft: weekly_soft(weekly.get("grok").copied().flatten()),
        claude_weekly_exhausted: weekly_hard(weekly.get("claude").copied().flatten()),
        codex_weekly_exhausted: weekly_hard(weekly.get("codex").copied().flatten()),
        opencode_weekly_exhausted: weekly_hard(weekly.get("opencode").copied().flatten()),
        kimi_weekly_exhausted: weekly_hard(weekly.get("kimi").copied().flatten()),
        grok_weekly_exhausted: weekly_hard(weekly.get("grok").copied().flatten()),
        fleet_scope: engines.keys().cloned().collect(),
        orch_reserved: std::env::var("NEOMAX_ORCH_RESERVED").ok().as_deref() == Some("1"),
        auth_rotations: Vec::new(),
        failover_events: Vec::new(),
        rotate_advised,
        orch_accounts,
        duplicate_accounts: engines
            .values()
            .flat_map(|view| view.accounts.iter())
            .filter(|account| !account.duplicate_of.is_empty())
            .filter_map(|account| account.email.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        quota_capabilities: quota_capabilities.into_iter().collect(),
        quota_available: quota_available.into_iter().collect(),
        quota_reactive: quota_reactive.into_iter().collect(),
    }
}

pub(crate) fn recent_rotations(source: &FilesystemPortalSource, now: i64) -> Result<Vec<Value>> {
    let log = RotationLog::new(source.paths().auth_rotations.clone());
    let cutoff = now.saturating_sub(ROTATION_WINDOW_SECONDS);
    let events = log
        .recent(ROTATION_LOG_READ_LIMIT)?
        .into_iter()
        .filter(|event| event.ts >= cutoff && event.ts <= now)
        .rev()
        .take(ROTATION_DISPLAY_LIMIT)
        .map(sanitize_rotation)
        .collect::<Vec<_>>();
    Ok(events.into_iter().rev().collect())
}

fn sanitize_rotation(event: RotationEvent) -> Value {
    let mut value = json!({
        "ts": event.ts,
        "engine": event.engine.as_str(),
        "operation": safe_operation(&event.operation),
        "destination": safe_profile_label(&event.destination),
    });
    if let Some(source) = event.source.as_deref() {
        value["source"] = safe_profile_label(source).into();
    }
    if let Some(reason) = event.reason.as_deref() {
        value["reason"] = safe_reason(reason).into();
    }
    value
}

fn safe_operation(operation: &str) -> &'static str {
    match operation {
        "copy" => "copy",
        "swap" => "swap",
        "restore" => "restore",
        _ => "other",
    }
}

fn safe_profile_label(value: &str) -> String {
    let candidate = Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("profile");
    let lower = candidate.to_ascii_lowercase();
    if candidate.is_empty()
        || candidate.len() > 80
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("bearer")
        || lower.contains("oauth")
        || lower.contains("credential")
        || !candidate.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
    {
        return "profile".into();
    }
    candidate.into()
}

fn safe_reason(reason: &str) -> &'static str {
    let reason = reason.to_ascii_lowercase();
    if reason.contains("quota")
        || reason.contains("limit")
        || reason.contains("429")
        || reason.contains("usage")
        || reason.contains("weekly")
        || reason.contains("5h")
        || reason.contains("five-hour")
        || reason.contains("five hour")
    {
        "quota"
    } else if reason.contains("manual") || reason.contains("rotate") {
        "manual"
    } else if reason.contains("handoff") || reason.contains("continu") {
        "handoff"
    } else if reason.contains("restore") {
        "restore"
    } else if reason.contains("maintenance") || reason.contains("tick") {
        "maintenance"
    } else {
        "other"
    }
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn weekly_soft(value: Option<f64>) -> bool {
    value.is_some_and(|value| value >= 99.0)
}

fn weekly_hard(value: Option<f64>) -> bool {
    value.is_some_and(|value| value >= 99.0)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::model::{AccountView, EngineCapabilitiesView};
    use neomax_core::Engine;

    #[test]
    fn weekly_flags_use_real_windows_for_every_provider() {
        let account = AccountView {
            n: "1".into(),
            email: Some("dev@example.test".into()),
            duplicate_of: vec!["1".into(), "2".into()],
            usage: Some(json!({
                "seven_day": {"used_percent": 99.0}
            })),
            capabilities: EngineCapabilitiesView {
                quota: crate::model::QuotaCapabilityView {
                    supported: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let engines = BTreeMap::from([(
            "opencode".into(),
            EngineView {
                accounts: vec![account],
                ..Default::default()
            },
        )]);
        let summary = build_summary(&engines, &[], &[], 0, 0);
        assert!(summary.opencode_weekly_soft);
        assert!(summary.opencode_weekly_exhausted);
        assert_eq!(summary.quota_capabilities, vec!["opencode"]);
        assert!(summary.quota_reactive.is_empty());
        assert_eq!(summary.duplicate_accounts, vec!["dev@example.test"]);
    }

    #[test]
    fn summary_separates_reactive_quota_signals_from_numeric_windows() {
        let account = AccountView {
            capabilities: EngineCapabilitiesView {
                quota: crate::model::QuotaCapabilityView {
                    reactive: true,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let engines = BTreeMap::from([(
            "grok".into(),
            EngineView {
                accounts: vec![account],
                ..Default::default()
            },
        )]);
        let summary = build_summary(&engines, &[], &[], 0, 0);
        assert!(summary.quota_capabilities.is_empty());
        assert_eq!(summary.quota_reactive, vec!["grok"]);
        assert!(summary.quota_available.is_empty());
    }

    #[test]
    fn recent_rotations_are_time_bounded_and_auth_safe() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        let log = RotationLog::new(source.paths().auth_rotations.clone());
        log.append(&RotationEvent {
            ts: 1_800_000_000 - 100,
            engine: Engine::Claude,
            operation: "swap".into(),
            destination: "/private/accounts/.claude-2".into(),
            source: Some("/private/accounts/.claude-1".into()),
            from_email: Some("from@example.test".into()),
            to_email: Some("to@example.test".into()),
            reason: Some("weekly 99%".into()),
        })
        .unwrap();
        log.append(&RotationEvent {
            ts: 1_800_000_000 - ROTATION_WINDOW_SECONDS - 1,
            engine: Engine::Codex,
            operation: "copy".into(),
            destination: "/private/old/.codex-1".into(),
            source: None,
            from_email: None,
            to_email: None,
            reason: Some("manual".into()),
        })
        .unwrap();
        log.append(&RotationEvent {
            ts: 1_800_000_000 + 1,
            engine: Engine::Opencode,
            operation: "swap".into(),
            destination: "/private/future/.opencode-1".into(),
            source: None,
            from_email: None,
            to_email: None,
            reason: Some("manual".into()),
        })
        .unwrap();

        let rotations = recent_rotations(&source, 1_800_000_000).unwrap();
        assert_eq!(rotations.len(), 1);
        assert_eq!(rotations[0]["engine"], "claude");
        assert_eq!(rotations[0]["operation"], "swap");
        assert_eq!(rotations[0]["destination"], ".claude-2");
        assert_eq!(rotations[0]["source"], ".claude-1");
        assert_eq!(rotations[0]["reason"], "quota");
        let serialized = serde_json::to_string(&rotations).unwrap();
        assert!(!serialized.contains("from_email"));
        assert!(!serialized.contains("from@example.test"));
        assert!(!serialized.contains("/private/accounts"));
        assert!(
            !fs::read_to_string(source.paths().auth_rotations.clone())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn unsafe_profile_labels_are_replaced_with_a_generic_value() {
        assert_eq!(safe_profile_label("/private/token=secret"), "profile");
        assert_eq!(safe_profile_label("/private/accounts/acct-3"), "acct-3");
        assert_eq!(safe_reason("5h 99% at or above 99%"), "quota");
        assert_eq!(safe_reason("weekly 99% at or above 99%"), "quota");
        assert_eq!(safe_reason("manual /rotate"), "manual");
        assert_eq!(safe_reason("opaque bearer token"), "other");
    }
}
