use super::super::{HandoffCheck, check_result, rotation_advice};
use crate::Engine;

#[test]
fn check_exit_semantics_match_the_cli_contract() {
    assert_eq!(
        check_result(Engine::Claude, "2", 50.0, 20.0, None, None, None).exit_code(),
        0
    );
    let check = check_result(
        Engine::Claude,
        "2",
        99.0,
        20.0,
        Some("3".into()),
        Some("~2d".into()),
        Some("target@example.test".into()),
    );
    assert!(check.advice.advised);
    assert_eq!(check.exit_code(), HandoffCheck::ROTATE_EXIT);
    assert_eq!(check.target_account.as_deref(), Some("3"));
}

#[test]
fn non_claude_providers_use_their_weekly_window() {
    for engine in [Engine::Codex, Engine::Opencode, Engine::Kimi, Engine::Grok] {
        assert!(!rotation_advice(engine, 99.0, 20.0).advised);
        assert!(rotation_advice(engine, 0.0, 99.0).advised);
    }
}
