use std::ffi::OsStr;

use super::support::request;
use crate::Engine;
use crate::providers::build_orchestrator_command;

#[test]
fn invalid_resume_and_profile_engine_fail_closed() {
    let resume_request = request(Engine::Claude).with_resume(true);
    assert!(
        build_orchestrator_command(Engine::Claude, OsStr::new("claude"), &resume_request).is_err()
    );
    let mut mismatched = request(Engine::Claude);
    mismatched.profile.engine = Engine::Codex;
    assert!(build_orchestrator_command(Engine::Claude, OsStr::new("claude"), &mismatched).is_err());
}

#[test]
fn secret_variables_are_not_copied_into_provider_commands() {
    let environment = request(Engine::Claude)
        .environment
        .with_variable("XAI_API_KEY", "fixture-secret");
    let request = crate::providers::OrchestratorRequest {
        environment,
        ..request(Engine::Claude)
    };
    let command =
        build_orchestrator_command(Engine::Claude, OsStr::new("claude"), &request).unwrap();
    assert!(!command.env.values().any(|value| value == "fixture-secret"));
}
