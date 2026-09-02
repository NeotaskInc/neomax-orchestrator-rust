use std::path::PathBuf;

use crate::sessions::artifacts::{artifact, ArtifactKind, MemoryArtifactSource};
use crate::sessions::filters::DiscoveryContext;
use crate::sessions::kimi::discover;

#[test]
fn kimi_state_and_wire_are_normalized_into_main_and_child_records() {
    let profile = PathBuf::from("/profile");
    let state_path = profile.join("sessions/s1/state.json");
    let source = MemoryArtifactSource::new([
        artifact(
            &profile,
            &state_path,
            ArtifactKind::KimiState,
            100,
            serde_json::json!({"sessionId":"s1","workDir":"/repo","createdAt":90,"agents":{"main":{"type":"main"},"agent-1":{"type":"sub","parentAgentId":"main"}}}).to_string().into_bytes(),
        ),
        artifact(
            &profile,
            profile.join("sessions/s1/agents/main/wire.jsonl"),
            ArtifactKind::KimiWire,
            100,
            b"{\"type\":\"usage.record\",\"model\":\"kimi-code/k3\",\"usage\":{\"inputOther\":4,\"output\":5}}\n".to_vec(),
        ),
        artifact(
            &profile,
            profile.join("sessions/s1/agents/agent-1/wire.jsonl"),
            ArtifactKind::KimiWire,
            99,
            b"{\"type\":\"usage.record\",\"model\":\"kimi-code/k3\",\"usage\":{\"inputOther\":2,\"output\":3}}\n".to_vec(),
        ),
    ]);
    let rows = discover(&source, &profile, "acct", &DiscoveryContext::new(100), 0).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|row| !row.is_child())
            .unwrap()
            .tokens
            .output,
        8
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.is_child())
            .unwrap()
            .tokens
            .output,
        3
    );
}
