use std::path::Path;

use crate::sessions::headers::{session_id_from_path, timestamp_epoch, workflow_id};

#[test]
fn identity_helpers_preserve_provider_path_rules() {
    assert_eq!(
        session_id_from_path(
            Path::new("rollout-0123456789abcdef0123456789abcdef0123456789.jsonl"),
            crate::Engine::Codex,
        ),
        "6789abcdef0123456789abcdef0123456789".to_string()
    );
    assert_eq!(
        workflow_id(Path::new("/tmp/workflows/wf_123/session.jsonl")),
        Some("wf_123".to_string())
    );
    assert_eq!(
        timestamp_epoch(&serde_json::json!("2026-08-23T00:00:00Z")),
        Some(1_787_443_200)
    );
    assert_eq!(
        timestamp_epoch(&serde_json::json!(1_787_443_200_000_i64)),
        Some(1_787_443_200)
    );
}
