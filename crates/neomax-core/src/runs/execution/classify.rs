use std::sync::OnceLock;

use regex::Regex;

use crate::providers::{ParsedEvents, TokenUsage};
use crate::runs::{RunRecord, RunStatus};

use super::types::{AttemptOutcome, KilledFor};

pub fn classify_attempt(
    exit_code: Option<i32>,
    parsed: &ParsedEvents,
    killed_for: Option<KilledFor>,
    stderr_tail: &str,
) -> RunStatus {
    match killed_for {
        Some(KilledFor::Timeout) => return RunStatus::Timeout,
        Some(KilledFor::Stalled) => return RunStatus::Stalled,
        Some(KilledFor::Quota) => return RunStatus::Limit,
        Some(KilledFor::Aborted) => return RunStatus::Aborted,
        None => {}
    }
    if parsed.rate_limited {
        return RunStatus::Limit;
    }
    let blob = format!(
        "{}\n{}\n{}",
        parsed.result_text.as_deref().unwrap_or_default(),
        stderr_tail,
        parsed.errors.join(" ")
    );
    let failed = parsed.is_error || exit_code != Some(0);
    if failed && interrupt_pattern().is_match(&blob) {
        return RunStatus::Aborted;
    }
    if failed && limit_pattern().is_match(&blob) {
        return RunStatus::Limit;
    }
    if failed || !matches!(parsed.subtype.as_deref(), None | Some("success")) {
        return RunStatus::Error;
    }
    RunStatus::Done
}

pub fn apply_outcome(run: &mut RunRecord, outcome: &AttemptOutcome, resumed: bool) {
    run.result_text = outcome.parsed.result_text.clone();
    if usage_present(&outcome.parsed.usage) {
        run.usage = serde_json::to_value(&outcome.parsed.usage).ok();
    }
    run.resets_at = outcome.parsed.resets_at.or(run.resets_at);
    if outcome.parsed.limit_window.is_some() {
        run.limit_window = outcome.parsed.limit_window.clone();
    }
    merge_children(run, &outcome.parsed);
    if let Some(session) = outcome.parsed.session_id.as_deref() {
        let new_provider_session = run.engine != crate::Engine::Claude && !resumed;
        let discovered_claude_session =
            run.engine == crate::Engine::Claude && run.session.is_none();
        if new_provider_session || discovered_claude_session {
            run.session = Some(session.into());
        }
    }
    if outcome.status == RunStatus::Error {
        run.error_detail = Some(format!(
            "{} rc={} {}",
            outcome.parsed.subtype.as_deref().unwrap_or_default(),
            outcome
                .exit_code
                .map_or_else(|| "signal".into(), |code| code.to_string()),
            trailing_chars(&outcome.stderr_tail, 400)
        ));
    } else if outcome.status == RunStatus::Limit && !outcome.parsed.errors.is_empty() {
        run.error_detail = Some(outcome.parsed.errors.join("; "));
    }
}

fn merge_children(run: &mut RunRecord, parsed: &ParsedEvents) {
    if parsed.children.is_empty() {
        return;
    }
    run.children.retain(|child| {
        child.get("attempt").and_then(serde_json::Value::as_u64) != Some(u64::from(run.attempt))
    });
    run.children
        .extend(parsed.children.iter().filter_map(|child| {
            let mut value = serde_json::to_value(child).ok()?;
            value
                .as_object_mut()?
                .insert("attempt".into(), run.attempt.into());
            Some(value)
        }));
}

fn usage_present(usage: &TokenUsage) -> bool {
    usage.input != 0
        || usage.output != 0
        || usage.reasoning != 0
        || usage.cache_read != 0
        || usage.cache_write != 0
        || usage.total != 0
        || usage.cost != 0.0
        || !usage.raw.is_empty()
}

fn trailing_chars(value: &str, count: usize) -> String {
    let mut chars = value.chars().rev().take(count).collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect()
}

fn limit_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new("(?i)usage limit|out of extra usage|rate.?limit|5-hour limit|weekly limit|credit balance is too low|tokens per min|\\bTPM\\b|requests per min|\\b429\\b|quota|credits? (?:depleted|exhausted)")
            .expect("static limit expression")
    })
}

fn interrupt_pattern() -> &'static Regex {
    static VALUE: OnceLock<Regex> = OnceLock::new();
    VALUE.get_or_init(|| {
        Regex::new("(?i)request was aborted|interrupted by user|all fibers interrupted")
            .expect("static interrupt expression")
    })
}

#[cfg(test)]
mod tests {
    use crate::providers::ChildActivity;

    use super::*;

    #[test]
    fn distinguishes_structured_limits_interruptions_and_errors() {
        let mut parsed = ParsedEvents {
            rate_limited: true,
            ..ParsedEvents::default()
        };
        assert_eq!(
            classify_attempt(Some(1), &parsed, None, ""),
            RunStatus::Limit
        );
        parsed.rate_limited = false;
        parsed.is_error = true;
        parsed.result_text = Some("request was aborted".into());
        assert_eq!(
            classify_attempt(Some(1), &parsed, None, ""),
            RunStatus::Aborted
        );
        parsed.result_text = Some("provider failed".into());
        assert_eq!(
            classify_attempt(Some(1), &parsed, None, ""),
            RunStatus::Error
        );
        assert_eq!(
            classify_attempt(Some(0), &ParsedEvents::default(), None, ""),
            RunStatus::Done
        );
    }

    #[test]
    fn applies_session_usage_and_attempt_scoped_children() {
        let mut run: RunRecord = serde_json::from_value(serde_json::json!({
            "id":"run", "engine":"opencode", "status":"running", "started":1,
            "attempt":2, "children":[{"id":"old","attempt":2},{"id":"kept","attempt":1}]
        }))
        .unwrap();
        let outcome = AttemptOutcome {
            status: RunStatus::Done,
            exit_code: Some(0),
            parsed: ParsedEvents {
                session_id: Some("session".into()),
                usage: TokenUsage {
                    output: 20,
                    ..TokenUsage::default()
                },
                children: vec![ChildActivity {
                    id: "new".into(),
                    kind: "agent".into(),
                    label: "work".into(),
                    status: "completed".into(),
                    last_tool: None,
                    tokens: 20,
                }],
                ..ParsedEvents::default()
            },
            stderr_tail: String::new(),
            log_path: "log".into(),
            stderr_path: "stderr".into(),
        };
        apply_outcome(&mut run, &outcome, false);
        assert_eq!(run.session.as_deref(), Some("session"));
        assert_eq!(run.usage.as_ref().unwrap()["output"], 20);
        assert_eq!(run.children.len(), 2);
        assert!(run.children.iter().any(|child| child["id"] == "new"));
    }

    #[test]
    fn accepts_a_new_codex_thread_after_a_fresh_attempt() {
        let mut run: RunRecord = serde_json::from_value(serde_json::json!({
            "id": "run",
            "engine": "codex",
            "status": "running",
            "started": 1,
            "session": "old-thread",
            "session_history": [{
                "session": "old-thread",
                "account": "codex-1",
                "attempt": 1
            }]
        }))
        .unwrap();
        run.session = None;
        let outcome = AttemptOutcome {
            status: RunStatus::Done,
            exit_code: Some(0),
            parsed: ParsedEvents {
                session_id: Some("new-thread".into()),
                ..ParsedEvents::default()
            },
            stderr_tail: String::new(),
            log_path: "log".into(),
            stderr_path: "stderr".into(),
        };

        apply_outcome(&mut run, &outcome, false);

        assert_eq!(run.session.as_deref(), Some("new-thread"));
        assert_eq!(run.session_history[0].session, "old-thread");
    }
}
