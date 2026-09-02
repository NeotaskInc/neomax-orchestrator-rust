use super::support::{args, request, CWD};
use crate::providers::catalog;
use crate::Engine;

#[test]
fn no_task_uses_interactive_shape() {
    assert_eq!(
        args(Engine::Opencode, &request(Engine::Opencode)),
        vec![
            CWD,
            "--model",
            catalog::OPENCODE_DEFAULT_MODEL,
            "--agent",
            "build",
            "--auto",
            "--prompt",
            crate::providers::worker::ORCHESTRATOR_DIRECTIVE,
        ]
    );
}

#[test]
fn goal_and_max_turns_are_encoded_in_the_opencode_prompt() {
    let request = request(Engine::Opencode)
        .with_goal("the tests pass")
        .with_max_turns(3);
    let prompt = args(Engine::Opencode, &request)
        .pop()
        .expect("OpenCode root prompt");
    assert!(prompt.contains("OBJECTIVE: do not finish until this condition holds:"));
    assert!(prompt.contains("Make at most 3 rounds of self-correction"));
}

#[test]
fn arbitrary_qualified_model_is_preserved() {
    let request = request(Engine::Opencode).with_model("local/opencode/custom");
    let args = args(Engine::Opencode, &request);
    assert!(args.contains(&"local/opencode/custom".into()));
}

#[test]
fn native_resume_without_a_follow_up_task_does_not_send_a_new_prompt() {
    let request = request(Engine::Opencode)
        .with_session("session-42")
        .with_resume(true);
    let args = args(Engine::Opencode, &request);
    assert!(args.windows(2).any(|pair| {
        pair.first().map(String::as_str) == Some("--session")
            && pair.get(1).map(String::as_str) == Some("session-42")
    }));
    assert!(!args.iter().any(|argument| argument == "--prompt"));
}
