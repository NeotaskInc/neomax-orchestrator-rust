use crate::sessions::activity::ActivityState;
use crate::sessions::headers::{claude_tail_activity, codex_session_live, codex_tail_activity};

#[test]
fn claude_tail_activity_ignores_bridge_metadata() {
    let tail = r#"{"type":"user","message":{"content":"work"}}
{"type":"assistant","message":{"stop_reason":"end_turn"}}
{"type":"bridge-session"}
{"type":"ai-title"}"#;
    assert_eq!(claude_tail_activity(tail, 100, 99, 60), ActivityState::Idle);
}

#[test]
fn codex_live_does_not_end_at_task_complete() {
    let tail = r#"{"type":"event_msg","payload":{"type":"task_complete"}}
{"type":"event_msg","payload":{"type":"token_count"}}"#;
    assert_eq!(codex_session_live(tail, 100, 99, 60), ActivityState::Active);
    assert_eq!(codex_tail_activity(tail, 100, 99, 60), ActivityState::Idle);
}
