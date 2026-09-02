use std::path::PathBuf;

use crate::sessions::artifacts::{artifact, ArtifactKind, MemoryArtifactSource};
use crate::sessions::codex::discover;
use crate::sessions::filters::DiscoveryContext;

#[test]
fn codex_discovery_keeps_live_orchestrator_rollout_at_task_complete() {
    let profile = PathBuf::from("/profile");
    let source = MemoryArtifactSource::new([artifact(
        &profile,
        "/profile/sessions/2026/08/rollout-aaaaaaaa-bbbb.jsonl",
        ArtifactKind::CodexRollout,
        99,
        br#"{"type":"session_meta","payload":{"cwd":"/repo"}}
{"type":"event_msg","payload":{"type":"user_message","message":"Fix it"}}
{"type":"event_msg","payload":{"type":"task_complete"}}
{"type":"event_msg","payload":{"type":"token_count","usage":{"input":3,"output":4}}}"#
            .to_vec(),
    )]);
    let rows = discover(&source, &profile, "acct", &DiscoveryContext::new(100), 0).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].active);
    assert!(!rows[0].working);
    assert_eq!(rows[0].label.as_deref(), Some("Fix it"));
    assert_eq!(rows[0].tokens.output, 4);
}
