use std::ffi::OsStr;

use super::support::{CWD, args, request};
use crate::Engine;
use crate::providers::{Provider, catalog};

#[test]
fn no_task_uses_interactive_shape() {
    assert_eq!(
        args(Engine::Claude, &request(Engine::Claude)),
        vec![
            "--model",
            catalog::CLAUDE_DEFAULT_MODEL,
            "--dangerously-skip-permissions",
            "--append-system-prompt",
            crate::providers::worker::ORCHESTRATOR_DIRECTIVE,
        ]
    );
}

#[test]
fn effort_and_ultra_use_provider_native_root_arguments() {
    let request = request(Engine::Claude).with_effort("max").with_ultra(true);
    let args = args(Engine::Claude, &request);
    assert_eq!(
        args.iter()
            .position(|argument| argument == "--effort")
            .and_then(|index| args.get(index + 1))
            .map(String::as_str),
        Some("max")
    );
    assert_eq!(
        args.iter()
            .position(|argument| argument == "--settings")
            .and_then(|index| args.get(index + 1))
            .map(String::as_str),
        Some(r#"{"ultracode":true}"#)
    );
}

#[test]
fn goal_and_max_turns_use_the_claude_startup_surface() {
    let request = request(Engine::Claude)
        .with_goal("the tests pass")
        .with_max_turns(3);
    let args = args(Engine::Claude, &request);
    assert_eq!(
        args.iter()
            .position(|argument| argument == "--max-turns")
            .and_then(|index| args.get(index + 1))
            .map(String::as_str),
        Some("3")
    );
    assert!(
        args.iter()
            .any(|argument| argument == "/goal the tests pass")
    );
}

#[test]
fn provider_adapter_keeps_the_typed_interactive_surface() {
    let command = crate::providers::Claude::new("fixture-claude")
        .orchestrator_command(&request(Engine::Claude))
        .unwrap();
    assert_eq!(command.program, OsStr::new("fixture-claude"));
    assert_eq!(command.cwd, std::path::PathBuf::from(CWD));
    assert!(!command.args.contains(&"-p".into()));
}
