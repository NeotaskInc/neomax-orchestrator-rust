use std::ffi::OsStr;

use super::support::request;
use crate::Engine;
use crate::providers::build_orchestrator_command;

#[test]
fn unsupported_provider_root_effort_and_ultra_are_rejected() {
    for engine in [Engine::Opencode, Engine::Kimi, Engine::Grok] {
        let effort_error = build_orchestrator_command(
            engine,
            OsStr::new(engine.as_str()),
            &request(engine).with_effort("high"),
        )
        .unwrap_err();
        assert!(
            effort_error.to_string().contains("do not support"),
            "{engine}"
        );

        let ultra_error = build_orchestrator_command(
            engine,
            OsStr::new(engine.as_str()),
            &request(engine).with_ultra(true),
        )
        .unwrap_err();
        assert!(
            ultra_error.to_string().contains("do not support"),
            "{engine}"
        );
    }
}

#[test]
fn root_model_validation_matches_provider_model_rules() {
    let whitespace = build_orchestrator_command(
        crate::Engine::Claude,
        OsStr::new("claude"),
        &request(crate::Engine::Claude).with_model("claude model"),
    )
    .unwrap_err();
    assert!(whitespace.to_string().contains("without whitespace"));

    let unqualified_opencode = build_orchestrator_command(
        crate::Engine::Opencode,
        OsStr::new("opencode"),
        &request(crate::Engine::Opencode).with_model("big-pickle"),
    )
    .unwrap_err();
    assert!(
        unqualified_opencode
            .to_string()
            .contains("provider/model form")
    );
}

#[test]
fn kimi_resume_rejects_a_new_initial_task() {
    let request = request(crate::Engine::Kimi)
        .with_session("session-42")
        .with_resume(true)
        .with_initial_task("follow-up task");
    let error = build_orchestrator_command(crate::Engine::Kimi, OsStr::new("kimi"), &request)
        .unwrap_err();
    assert!(error.to_string().contains("cannot combine a new initial task"));
}
