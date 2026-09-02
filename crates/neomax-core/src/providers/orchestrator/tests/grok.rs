use super::support::{CWD, args, request};
use crate::Engine;
use crate::providers::catalog;

#[test]
fn no_task_uses_interactive_shape() {
    assert_eq!(
        args(Engine::Grok, &request(Engine::Grok)),
        vec![
            "--no-auto-update",
            "--model",
            catalog::GROK_DEFAULT_MODEL,
            "--always-approve",
            "--rules",
            crate::providers::worker::ORCHESTRATOR_DIRECTIVE,
            "--cwd",
            CWD,
        ]
    );
}

#[test]
fn goal_and_max_turns_use_the_grok_startup_surface() {
    let request = request(Engine::Grok)
        .with_goal("the tests pass")
        .with_max_turns(3);
    let args = args(Engine::Grok, &request);
    assert!(args.contains(&"--max-turns".into()));
    assert!(
        args.last()
            .is_some_and(|prompt| prompt.contains("the tests pass"))
    );
}
