use std::path::{Path, PathBuf};

use crate::sessions::artifacts::{artifact, ArtifactKind, MemoryArtifactSource};
use crate::sessions::claude::discover;
use crate::sessions::filters::DiscoveryContext;
use crate::sessions::types::SessionKind;

#[test]
fn discovers_claude_main_and_subagent_with_project_and_usage() {
    let profile = PathBuf::from("/profile");
    let source = MemoryArtifactSource::new([
        artifact(
            &profile,
            "/profile/projects/p/session.jsonl",
            ArtifactKind::ClaudeMain,
            95,
            br#"{"type":"user","sessionId":"main","cwd":"/repo","gitBranch":"main","message":{"content":"Build it"},"timestamp":95}
{"type":"assistant","message":{"usage":{"input_tokens":5,"output_tokens":7},"stop_reason":"tool_use"}}"#.to_vec(),
        ),
        artifact(
            &profile,
            "/profile/projects/p/main/subagents/agent-a.jsonl",
            ArtifactKind::ClaudeSubagent,
            94,
            br#"{"type":"user","sessionId":"main","cwd":"/repo","message":{"content":"Inspect"}}
{"type":"assistant","message":{"stop_reason":"end_turn"}}"#.to_vec(),
        ),
    ]);
    let context = DiscoveryContext::new(100)
        .with_project_resolver(|path: &Path| (path == Path::new("/repo")).then(|| "repo".into()));
    let rows = discover(&source, &profile, "acct", &context, 0).unwrap();
    assert_eq!(rows.len(), 2);
    let main = rows
        .iter()
        .find(|row| row.kind == SessionKind::Main)
        .unwrap();
    assert_eq!(main.project.as_deref(), Some("repo"));
    assert_eq!(main.tokens.output, 7);
    assert!(main.active);
    assert_eq!(rows.iter().filter(|row| row.is_child()).count(), 1);
}
