use anyhow::Result;
use neomax_core::runs::{EventStore, RunEvent};
use serde_json::{Value, json};

use super::FilesystemPortalSource;

const MAX_FAILOVER_EVENTS: usize = 200;
const FAILOVER_EVENT_NAMES: &[&str] = &[
    "failover",
    "cross_provider_failover",
    "handoff",
    "rotation",
    "rotate",
];

pub(crate) fn read_failover_events(
    source: &FilesystemPortalSource,
    now: i64,
) -> Result<Vec<Value>> {
    let events = EventStore::with_legacy_directory(
        source.paths.run_events.clone(),
        source.paths.events.clone(),
    )
    .read(None, MAX_FAILOVER_EVENTS.saturating_mul(4))?;
    let cutoff = now.saturating_sub(30 * 86_400);
    let mut events = events
        .into_iter()
        .filter(|event| event.ts >= cutoff && event.ts <= now)
        .filter(|event| FAILOVER_EVENT_NAMES.contains(&event.event.as_str()))
        .map(sanitize_event)
        .collect::<Vec<_>>();
    if events.len() > MAX_FAILOVER_EVENTS {
        let keep_from = events.len() - MAX_FAILOVER_EVENTS;
        events.drain(..keep_from);
    }
    Ok(events)
}

fn sanitize_event(event: RunEvent) -> Value {
    let mut value = json!({
        "ts": event.ts,
        "run": event.run,
        "event": safe_event(&event.event),
        "engine": event.engine.as_str(),
        "account": safe_label(event.account.as_deref()),
        "status": event.status.map(|status| status.as_str()),
        "attempt": event.attempt,
    });
    for key in [
        "reason",
        "strategy",
        "to",
        "to_account",
        "to_engine",
        "target_engine",
        "from_account",
    ] {
        if let Some(value_to_copy) = event.extra.get(key).and_then(safe_extra_value) {
            value[key] = value_to_copy;
        }
    }
    value
}

fn safe_event(value: &str) -> &'static str {
    match value {
        "failover" => "failover",
        "cross_provider_failover" => "cross_provider_failover",
        "handoff" => "handoff",
        "rotation" => "rotation",
        "rotate" => "rotate",
        _ => "other",
    }
}

fn safe_label(value: Option<&str>) -> Value {
    value
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 120
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(character, '.' | '_' | '-' | '/' | ':')
                })
        })
        .map_or(Value::Null, |value| Value::String(value.into()))
}

fn safe_extra_value(value: &Value) -> Option<Value> {
    match value {
        Value::String(value) => safe_label(Some(value))
            .is_string()
            .then_some(value.clone().into()),
        Value::Number(_) | Value::Bool(_) | Value::Null => Some(value.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use chrono::Utc;
    use neomax_core::Engine;

    #[test]
    fn reads_failover_and_handoff_events_without_private_extra_fields() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        let now = Utc::now().timestamp();
        let event = RunEvent {
            ts: now - 1,
            run: "run-1".into(),
            event: "cross_provider_failover".into(),
            engine: Engine::Claude,
            account: Some("acct-1".into()),
            status: Some(neomax_core::runs::RunStatus::Limit),
            attempt: Some(2),
            extra: BTreeMap::from([
                ("to_engine".into(), json!("grok")),
                ("to_account".into(), json!("acct-2")),
                ("reason".into(), json!("quota")),
                ("profile_path".into(), json!("/private/token=secret")),
            ]),
        };
        let at = chrono::DateTime::from_timestamp(now - 1, 0).unwrap();
        EventStore::with_legacy_directory(
            source.paths.run_events.clone(),
            source.paths.events.clone(),
        )
        .append(&event, at)
        .unwrap();
        let events = read_failover_events(&source, now).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["event"], "cross_provider_failover");
        assert_eq!(events[0]["to_engine"], "grok");
        assert!(events[0].get("profile_path").is_none());
    }

    #[test]
    fn malformed_or_missing_event_state_is_an_empty_optional_view() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        std::fs::create_dir_all(&source.paths.run_events).unwrap();
        std::fs::write(source.paths.run_events.join("broken.jsonl"), b"{\n").unwrap();
        assert!(
            read_failover_events(&source, 1_800_000_000)
                .unwrap()
                .is_empty()
        );
    }
}
