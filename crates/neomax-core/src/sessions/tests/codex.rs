use std::path::PathBuf;

use crate::sessions::artifacts::{ArtifactKind, MemoryArtifactSource, artifact};
use crate::sessions::codex::discover;
use crate::sessions::filters::DiscoveryContext;

#[test]
fn codex_root_is_not_reparented_by_later_metadata_and_late_task_is_found() {
    let profile = PathBuf::from("/profile");
    let content = [
        serde_json::json!({"type":"session_meta","timestamp":"2026-09-07T09:00:00Z","payload":{"id":"root","cwd":"/repo"}}),
        serde_json::json!({"type":"session_meta","payload":{"id":"child","parent_thread_id":"root"}}),
        serde_json::json!({"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"x".repeat(300_000)}]}}),
        serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":"Repair the invoice retry"}}),
        serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"a","arguments":"cargo test invoice"}}),
        serde_json::json!({"type":"event_msg","payload":{"type":"agent_message","message":"The retry test passes."}}),
    ].into_iter().map(|event| event.to_string()).collect::<Vec<_>>().join("\n");
    let source = MemoryArtifactSource::new([artifact(
        &profile,
        "/profile/sessions/rollout-root.jsonl",
        ArtifactKind::CodexRollout,
        99,
        content.into_bytes(),
    )]);
    let rows = discover(&source, &profile, "1", &DiscoveryContext::new(100), 0).unwrap();
    assert_eq!(rows[0].id, "root");
    assert_eq!(rows[0].parent_id, None);
    let activity = rows[0].activity.as_ref().unwrap();
    assert_eq!(
        activity.task.as_ref().unwrap().text,
        "Repair the invoice retry"
    );
    assert_eq!(
        activity.last_message.as_ref().unwrap().text,
        "The retry test passes."
    );
    assert_eq!(activity.last_tool.as_ref().unwrap().name, "exec_command");
}

#[test]
fn codex_native_thread_metadata_preserves_parent_and_child_identity() {
    let profile = PathBuf::from("/profile");
    let source = MemoryArtifactSource::new([artifact(&profile, "/profile/sessions/rollout-child.jsonl", ArtifactKind::CodexRollout, 99,
        br#"{"type":"session_meta","payload":{"id":"child-id","cwd":"/repo","source":{"subagent":{"thread_spawn":{"parent_thread_id":"parent-id","depth":1}}}}}"#.to_vec())]);
    let rows = discover(&source, &profile, "2", &DiscoveryContext::new(100), 0).unwrap();
    assert_eq!(rows[0].id, "child-id");
    assert_eq!(rows[0].parent_id.as_deref(), Some("parent-id"));
    assert!(rows[0].is_child());
}

#[test]
fn codex_native_token_count_uses_latest_cumulative_totals_without_double_counting_cache() {
    let profile = PathBuf::from("/profile");
    let source = MemoryArtifactSource::new([artifact(&profile, "/profile/sessions/rollout-native.jsonl", ArtifactKind::CodexRollout, 99,
        br#"{"type":"session_meta","payload":{"id":"native","cwd":"/repo"}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":30,"output_tokens":20,"total_tokens":120}}}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":200,"cached_input_tokens":80,"output_tokens":30,"reasoning_output_tokens":10,"total_tokens":230}}}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":200,"cached_input_tokens":80,"output_tokens":30,"reasoning_output_tokens":10,"total_tokens":230}}}}"#.to_vec())]);
    let rows = discover(&source, &profile, "1", &DiscoveryContext::new(100), 0).unwrap();
    assert_eq!(rows[0].tokens.total, 230);
    assert_eq!(rows[0].tokens.input, 120);
    assert_eq!(rows[0].tokens.cache_read, 80);
    assert_eq!(rows[0].tokens.output, 30);
    assert_eq!(rows[0].tokens.reasoning, 10);
}

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
