use crate::sessions::headers::{claude_head_meta, codex_head_meta};

#[test]
fn claude_metadata_skips_system_prompt_and_trailing_metadata() {
    let head = r#"{"type":"summary"}
{"type":"user","cwd":"/repo","gitBranch":"main","slug":"slug","timestamp":"2026-08-23T00:00:00Z","message":{"content":"<system-reminder>noise</system-reminder>"}}
{"type":"user","sessionId":"sess-1","message":{"content":[{"type":"text","text":"Build the portal"}]}}"#;
    let meta = claude_head_meta(head);
    assert_eq!(meta.cwd.as_deref(), Some("/repo"));
    assert_eq!(meta.branch.as_deref(), Some("main"));
    assert_eq!(meta.label.as_deref(), Some("Build the portal"));
    assert_eq!(meta.started, Some(1_787_443_200));
}

#[test]
fn codex_metadata_reads_nested_session_fields() {
    let meta =
        codex_head_meta(r#"{"type":"session_meta","payload":{"cwd":"/repo","branch":"main"}}"#);
    assert_eq!(meta.cwd.as_deref(), Some("/repo"));
    assert_eq!(meta.branch.as_deref(), Some("main"));
}
