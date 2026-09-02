use serde_json::Value;

use crate::sessions::activity::{classify_activity, ActivityInput, ActivityState};
use crate::sessions::artifacts::json_lines;

pub fn claude_tail_activity(tail: &str, now: i64, modified: i64, window: i64) -> ActivityState {
    let mut state = ActivityState::Unknown;
    for event in json_lines(tail).rev() {
        match event.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                let stop = event
                    .get("message")
                    .and_then(|message| message.get("stop_reason"))
                    .and_then(Value::as_str);
                state = classify_activity(ActivityInput {
                    now,
                    last_modified: modified,
                    active_window: window,
                    terminal: stop == Some("end_turn"),
                    progress: true,
                    ..ActivityInput::default()
                });
                break;
            }
            Some("user") => {
                state = classify_activity(ActivityInput {
                    now,
                    last_modified: modified,
                    active_window: window,
                    progress: true,
                    ..ActivityInput::default()
                });
                break;
            }
            _ => {}
        }
    }
    state
}

pub fn codex_tail_activity(tail: &str, now: i64, modified: i64, window: i64) -> ActivityState {
    const IDLE: &[&str] = &[
        "task_complete",
        "turn.completed",
        "turn.failed",
        "turn_aborted",
        "shutdown_complete",
        "session_end",
    ];
    const BUSY: &[&str] = &[
        "task_started",
        "turn.started",
        "user_message",
        "exec_command_begin",
        "exec_approval_request",
        "item.started",
        "item.updated",
        "agent_reasoning_delta",
        "agent_message_delta",
        "mcp_tool_call_begin",
    ];
    for event in json_lines(tail).rev() {
        let payload = event.get("payload").unwrap_or(&event);
        let kind = payload
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| event.get("type").and_then(Value::as_str));
        let Some(kind) = kind else {
            continue;
        };
        if IDLE.contains(&kind) {
            return ActivityState::Idle;
        }
        if BUSY.contains(&kind) {
            return classify_activity(ActivityInput {
                now,
                last_modified: modified,
                active_window: window,
                progress: true,
                ..ActivityInput::default()
            });
        }
    }
    ActivityState::Unknown
}

pub fn codex_session_live(tail: &str, now: i64, modified: i64, window: i64) -> ActivityState {
    for event in json_lines(tail).rev() {
        let payload = event.get("payload").unwrap_or(&event);
        let kind = payload
            .get("type")
            .and_then(Value::as_str)
            .or_else(|| event.get("type").and_then(Value::as_str));
        let Some(kind) = kind else {
            continue;
        };
        let live = !matches!(kind, "shutdown_complete" | "session_end");
        return classify_activity(ActivityInput {
            now,
            last_modified: modified,
            active_window: window,
            live: Some(live),
            ..ActivityInput::default()
        });
    }
    ActivityState::Unknown
}
