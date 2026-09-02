use std::collections::BTreeMap;

use neomax_core::config::Engine;
use neomax_core::sessions::SessionRecord;
use serde_json::{Value, json};

pub(crate) use crate::source::SESSION_ACTIVITY_WINDOW_SECONDS as LIVE_SESSION_WINDOW_SECONDS;

pub(crate) fn telemetry_for(
    engine: Engine,
    account: &str,
    sessions: &[SessionRecord],
    window_days: u32,
) -> Option<Value> {
    // Claude and Codex quota state comes from their provider integrations. Local
    // transcript totals are history, not a supported quota signal.
    if matches!(engine, Engine::Claude | Engine::Codex) {
        return None;
    }
    let rows = sessions
        .iter()
        .filter(|row| row.engine == engine && row.account == account)
        .collect::<Vec<_>>();
    let mut totals = BTreeMap::<&str, u64>::new();
    let cost = rows
        .iter()
        .map(|row| row.tokens.cost)
        .fold(0.0, |total, value| total + value);
    for row in &rows {
        add(&mut totals, "in", row.tokens.input);
        add(&mut totals, "out", row.tokens.output);
        add(&mut totals, "reasoning", row.tokens.reasoning);
        add(&mut totals, "cw", row.tokens.cache_write);
        add(&mut totals, "cr", row.tokens.cache_read);
        add(&mut totals, "requests", row.requests);
        add(&mut totals, "completions", row.completions);
        add(&mut totals, "errors", row.errors);
        add(&mut totals, "rate_limits", row.rate_limits);
        add(&mut totals, "tool_calls", row.tool_calls);
        add(&mut totals, "tool_errors", row.tool_errors);
        add(&mut totals, "files", row.files.len() as u64);
        add(
            &mut totals,
            "adds",
            saturating_sum(row.files.iter().map(|file| file.adds)),
        );
        add(
            &mut totals,
            "dels",
            saturating_sum(row.files.iter().map(|file| file.dels)),
        );
    }
    totals.insert("sessions", rows.len() as u64);
    totals.insert(
        "main_sessions",
        rows.iter().filter(|row| !row.is_child()).count() as u64,
    );
    totals.insert(
        "native_subagents",
        rows.iter().filter(|row| row.is_child()).count() as u64,
    );
    let last_activity = rows
        .iter()
        .filter_map(|row| row.last_active)
        .max()
        .unwrap_or_default();
    Some(json!({
        "available": !rows.is_empty(),
        "source": "session-artifacts",
        "account": account,
        "window_days": window_days,
        "totals": totals,
        "cost": cost,
        "last_activity": last_activity,
    }))
}

pub(crate) fn is_live_main(record: &SessionRecord, now: i64) -> bool {
    !record.is_child()
        && !record.archived
        && !record.done
        && record.active
        && is_recent(record.last_active, now)
}

pub(crate) fn is_working_subagent(record: &SessionRecord, now: i64) -> bool {
    record.is_child()
        && !record.archived
        && !record.done
        && record.working
        && is_recent(record.last_active, now)
}

fn is_recent(last_active: Option<i64>, now: i64) -> bool {
    last_active.is_some_and(|last| now.saturating_sub(last) <= LIVE_SESSION_WINDOW_SECONDS)
}

fn add(totals: &mut BTreeMap<&str, u64>, key: &'static str, value: u64) {
    let entry = totals.entry(key).or_default();
    *entry = entry.saturating_add(value);
}

fn saturating_sum(values: impl Iterator<Item = u64>) -> u64 {
    values.fold(0, |total, value| total.saturating_add(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::sessions::FileActivity;

    #[test]
    fn empty_sessions_report_unavailable_telemetry() {
        let value = telemetry_for(Engine::Grok, "1", &[], 3).unwrap();
        assert_eq!(value["available"], false);
        assert_eq!(value["totals"]["sessions"], 0);
        assert_eq!(value["window_days"], 3);
    }

    #[test]
    fn file_additions_and_deletions_saturate_at_u64_max() {
        let mut session = SessionRecord::with_identity("session", Engine::Grok, "1");
        session.files = vec![
            FileActivity {
                adds: u64::MAX,
                dels: u64::MAX,
                ..FileActivity::default()
            },
            FileActivity {
                adds: 1,
                dels: 1,
                ..FileActivity::default()
            },
        ];

        let value = telemetry_for(Engine::Grok, "1", &[session], 7).unwrap();
        assert_eq!(value["totals"]["adds"], u64::MAX);
        assert_eq!(value["totals"]["dels"], u64::MAX);
    }

    #[test]
    fn session_cost_is_preserved_alongside_token_and_request_totals() {
        let mut session = SessionRecord::with_identity("session", Engine::Grok, "1");
        session.tokens.cost = 1.25;
        session.requests = 2;
        session.completions = 1;
        let value = telemetry_for(Engine::Grok, "1", &[session], 30).unwrap();
        assert_eq!(value["cost"], 1.25);
        assert_eq!(value["totals"]["requests"], 2);
        assert_eq!(value["totals"]["completions"], 1);
    }

    #[test]
    fn provider_quota_integrations_are_not_represented_as_local_telemetry() {
        let session = SessionRecord::with_identity("session", Engine::Claude, "1");
        assert!(telemetry_for(Engine::Claude, "1", std::slice::from_ref(&session), 7).is_none());
        assert!(telemetry_for(Engine::Codex, "1", std::slice::from_ref(&session), 7).is_none());
    }

    #[test]
    fn live_counts_require_active_recent_main_or_working_recent_child() {
        let now = 1_800_000_000;
        let mut main = SessionRecord::with_identity("main", Engine::Opencode, "1");
        main.active = true;
        main.last_active = Some(now - LIVE_SESSION_WINDOW_SECONDS);
        assert!(is_live_main(&main, now));

        main.last_active = Some(now - LIVE_SESSION_WINDOW_SECONDS - 1);
        assert!(!is_live_main(&main, now));

        main.last_active = Some(now);
        main.archived = true;
        assert!(!is_live_main(&main, now));

        let mut child = SessionRecord::with_identity("child", Engine::Opencode, "1");
        child.parent_id = Some(main.id.clone());
        child.working = true;
        child.last_active = Some(now - LIVE_SESSION_WINDOW_SECONDS);
        assert!(is_working_subagent(&child, now));

        child.last_active = Some(now - LIVE_SESSION_WINDOW_SECONDS - 1);
        assert!(!is_working_subagent(&child, now));
    }
}
