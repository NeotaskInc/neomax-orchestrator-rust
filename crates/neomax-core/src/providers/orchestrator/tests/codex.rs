use super::support::{args, request, CWD};
use crate::providers::catalog;
use crate::Engine;

#[test]
fn no_task_uses_interactive_shape() {
    assert_eq!(
        args(Engine::Codex, &request(Engine::Codex)),
        vec![
            "-m",
            catalog::CODEX_DEFAULT_MODEL,
            "-c",
            "service_tier=fast",
            "-a",
            "never",
            "-s",
            "danger-full-access",
            "-C",
            CWD,
            crate::providers::worker::ORCHESTRATOR_DIRECTIVE,
        ]
    );
}

#[test]
fn effort_and_ultra_use_provider_native_root_arguments() {
    let effort_request = request(Engine::Codex).with_effort("medium");
    assert!(args(Engine::Codex, &effort_request).contains(&"model_reasoning_effort=medium".into()));
    let ultra_request = request(Engine::Codex).with_ultra(true);
    assert!(args(Engine::Codex, &ultra_request).contains(&"model_reasoning_effort=xhigh".into()));
}

#[test]
fn goal_and_max_turns_are_encoded_in_the_codex_prompt() {
    let request = request(Engine::Codex)
        .with_goal("the tests pass")
        .with_max_turns(3);
    let prompt = args(Engine::Codex, &request)
        .pop()
        .expect("Codex root prompt");
    assert!(prompt.contains("OBJECTIVE: do not finish until this condition holds:"));
    assert!(prompt.contains("the tests pass"));
    assert!(prompt.contains("Make at most 3 rounds of self-correction"));
}

#[test]
fn native_resume_without_a_follow_up_task_does_not_send_a_new_prompt() {
    let request = request(Engine::Codex)
        .with_session("session-42")
        .with_resume(true);
    let args = args(Engine::Codex, &request);
    assert!(args.windows(2).any(|pair| {
        pair.first().map(String::as_str) == Some("resume")
            && pair.get(1).map(String::as_str) == Some("session-42")
    }));
    assert!(!args
        .iter()
        .any(|argument| argument.contains("You are the Neomax orchestrator")));
}
