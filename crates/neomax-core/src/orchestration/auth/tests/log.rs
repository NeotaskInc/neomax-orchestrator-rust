use std::fs;
use std::path::Path;

use crate::orchestration::auth::limits::MAX_ROTATION_LOG_BYTES;
use crate::orchestration::auth::{RotationEvent, RotationEventContext, RotationLog};
use crate::Engine;

#[test]
fn appends_structured_events_without_credential_fields() {
    let temp = tempfile::tempdir().unwrap();
    let log = RotationLog::new(temp.path().join("rotations.jsonl"));
    log.append(&RotationEvent::from_context(RotationEventContext {
        ts: 10,
        engine: Engine::Claude,
        operation: "copy",
        destination: Path::new(".claude-2"),
        source: Some(Path::new(".claude-1")),
        from_email: Some("old@example.test".into()),
        to_email: Some("new@example.test".into()),
        reason: Some("quota".into()),
    }))
    .unwrap();
    let text = fs::read_to_string(log.path()).unwrap();
    assert!(!text.contains("accessToken"));
    assert!(text.contains("\"destination\":\".claude-2\""));
    assert!(text.contains("\"dest\":\".claude-2\""));
    assert!(text.contains("\"source\":\".claude-1\""));
    assert!(text.contains("\"src\":\".claude-1\""));
    assert_eq!(log.recent(10).unwrap().len(), 1);
}

#[test]
fn reads_legacy_event_without_operation_or_native_field_names() {
    let temp = tempfile::tempdir().unwrap();
    let log = RotationLog::new(temp.path().join("rotations.jsonl"));
    fs::write(
        log.path(),
        br#"{"ts":10,"engine":"claude","dest":".claude-2","src":".claude-1","reason":"quota"}
"#,
    )
    .unwrap();

    let event = log.recent(10).unwrap().pop().unwrap();
    assert_eq!(event.operation, "legacy");
    assert_eq!(event.destination, ".claude-2");
    assert_eq!(event.source.as_deref(), Some(".claude-1"));
    assert_eq!(event.reason.as_deref(), Some("quota"));
}

#[test]
fn conflicting_rotation_field_aliases_are_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let log = RotationLog::new(temp.path().join("rotations.jsonl"));
    fs::write(
        log.path(),
        br#"{"ts":10,"engine":"claude","destination":".claude-2","dest":".claude-3"}
"#,
    )
    .unwrap();

    assert!(log.recent(10).unwrap().is_empty());
}

#[test]
fn malformed_lines_are_ignored_without_poisoning_the_log_reader() {
    let temp = tempfile::tempdir().unwrap();
    let log = RotationLog::new(temp.path().join("rotations.jsonl"));
    fs::write(log.path(), b"not-json\n").unwrap();
    assert!(log.recent(10).unwrap().is_empty());
}

#[test]
fn oversized_rotation_log_is_rejected_before_parsing() {
    let temp = tempfile::tempdir().unwrap();
    let log = RotationLog::new(temp.path().join("rotations.jsonl"));
    fs::write(log.path(), vec![b'x'; MAX_ROTATION_LOG_BYTES + 1]).unwrap();
    assert!(log.recent(10).is_err());
}
