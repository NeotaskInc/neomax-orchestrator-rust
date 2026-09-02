use std::path::PathBuf;

use crate::sessions::artifacts::{artifact, ArtifactKind, MemoryArtifactSource};
use crate::sessions::filters::DiscoveryContext;
use crate::sessions::grok::{discover, extract_usage};

#[test]
fn grok_updates_normalize_usage_and_native_agent() {
    let profile = PathBuf::from("/profile");
    let source = MemoryArtifactSource::new([
        artifact(
            &profile,
            "/profile/sessions/s1/summary.json",
            ArtifactKind::GrokSummary,
            100,
            serde_json::json!({"info":{"id":"s1","cwd":"/repo"},"created_at":90,"current_model_id":"grok-4.6"}).to_string().into_bytes(),
        ),
        artifact(
            &profile,
            "/profile/sessions/s1/updates.jsonl",
            ArtifactKind::GrokUpdates,
            100,
            b"{\"timestamp\":100,\"params\":{\"update\":{\"sessionUpdate\":\"subagent_spawned\",\"subagent_id\":\"a1\",\"description\":\"inspect\"}}}\n{\"timestamp\":101,\"params\":{\"update\":{\"sessionUpdate\":\"turn_completed\",\"usage\":{\"inputTokens\":10,\"outputTokens\":20,\"cachedReadTokens\":2}}}}\n".to_vec(),
        ),
    ]);
    let rows = discover(&source, &profile, "acct", &DiscoveryContext::new(102), 0).unwrap();
    assert_eq!(rows.len(), 2);
    let main = rows.iter().find(|row| !row.is_child()).unwrap();
    assert_eq!(main.tokens.input, 8);
    assert_eq!(main.tokens.output, 20);
    assert_eq!(
        rows.iter()
            .find(|row| row.is_child())
            .unwrap()
            .label
            .as_deref(),
        Some("inspect")
    );
}

#[test]
fn grok_usage_api_extracts_stable_turn_records() {
    let text = r#"{"timestamp":101,"params":{"update":{"sessionUpdate":"turn_completed","prompt_id":"turn-1","stop_reason":"end_turn","usage":{"inputTokens":12,"outputTokens":20,"cachedReadTokens":2,"cacheCreationTokens":1,"reasoningTokens":4,"modelCalls":2,"modelUsage":{"grok-4.6":{}}}}}}"#;
    let rows = extract_usage(text, "s1", Some("grok-default"), 90);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "turn-1");
    assert_eq!(rows[0].session_id, "s1");
    assert_eq!(rows[0].tokens.input, 9);
    assert_eq!(rows[0].tokens.output, 20);
    assert_eq!(rows[0].tokens.cache_read, 2);
    assert_eq!(rows[0].requests, 2);
    assert_eq!(rows[0].completions, 1);
    assert_eq!(rows[0].model.as_deref(), Some("grok-4.6"));
}
