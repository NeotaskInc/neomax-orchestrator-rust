use neomax_core::Engine;
use neomax_core::sessions::{
    ActivityInput, ActivityState, PortalSnapshot, SessionKind, SessionRecord, portal_snapshot,
};

use super::support::{assert_fixture_is_sanitized, fixture_as, fixture_json};

#[test]
fn portal_fixture_preserves_all_provider_session_fields_and_unknown_fields() {
    assert_fixture_is_sanitized("sessions/portal.json");
    let expected = fixture_json("sessions/portal.json");
    let portal: PortalSnapshot = serde_json::from_value(expected).unwrap();
    assert_eq!(portal.generated_at, 1_787_488_123);
    assert_eq!(portal.sessions.len(), 1);
    assert_eq!(portal.subagents.len(), 1);
    assert_eq!(portal.sessions[0].engine, Engine::Codex);
    assert_eq!(portal.sessions[0].tokens.extra["future_token_field"], true);
    assert_eq!(portal.sessions[0].extra["future_session_field"], "preserve");
    assert_eq!(portal.sessions[0].files[0].extra["future_file_field"], true);
    assert_eq!(portal.summary.rate_limits, 1);
}

#[test]
fn portal_snapshot_derives_mains_children_age_and_summary() {
    let fixture: PortalSnapshot = fixture_as("sessions/portal.json");
    let main = fixture.sessions[0].clone();
    let child = fixture.subagents[0].clone();
    let snapshot = portal_snapshot(fixture.generated_at, [main, child]);
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.subagents.len(), 1);
    assert_eq!(snapshot.sessions[0].age_s, Some(3));
    assert_eq!(snapshot.summary.input, 1200);
    assert_eq!(snapshot.summary.output, 920);
    assert_eq!(snapshot.summary.active, 1);
    assert_eq!(snapshot.summary.working, 1);
}

#[test]
fn session_unknown_fields_and_legacy_child_values_are_retained() {
    let parent = SessionRecord::with_identity("parent", Engine::Kimi, "kimi-1");
    let child = neomax_core::sessions::subagents::child_from_value(
        &serde_json::json!({
            "id": "child",
            "status": "running",
            "created_at": 10,
            "updated_at": 20,
            "tokens": {"input_tokens": 4, "output_tokens": 5},
            "files": [{"file_path":"src/lib.rs","additions":2,"deletions":1}]
        }),
        &parent,
        "child",
        Some("review".into()),
    );
    assert_eq!(child.kind, SessionKind::NativeSubagent);
    assert_eq!(child.parent_id.as_deref(), Some("parent"));
    assert_eq!(child.tokens.input, 4);
    assert_eq!(child.tokens.output, 5);
    assert_eq!(child.files[0].path, "src/lib.rs");
    assert_eq!(child.files[0].adds, 2);
}

#[test]
fn activity_classification_is_provider_neutral_and_missing_state_is_unknown() {
    assert_eq!(
        neomax_core::sessions::classify_activity(ActivityInput {
            now: 100,
            last_modified: 99,
            active_window: 60,
            progress: true,
            ..ActivityInput::default()
        }),
        ActivityState::Active
    );
    assert_eq!(
        neomax_core::sessions::classify_activity(ActivityInput {
            now: 100,
            last_modified: 1,
            active_window: 10,
            ..ActivityInput::default()
        }),
        ActivityState::Unknown
    );
}
