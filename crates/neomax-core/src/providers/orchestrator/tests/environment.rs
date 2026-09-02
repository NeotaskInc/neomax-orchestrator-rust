use std::ffi::OsStr;
use std::path::PathBuf;

use super::support::{HOME, request};
use crate::Engine;
use crate::providers::{ORCHESTRATOR_INSTRUCTION_ENV, build_orchestrator_command, catalog};

#[test]
fn full_environment_is_explicit_and_provider_models_are_preserved() {
    let request = request(Engine::Opencode).with_model("local/opencode/custom");
    let command =
        build_orchestrator_command(Engine::Opencode, OsStr::new("opencode"), &request).unwrap();
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_ROLE")),
        Some(&"opencode".into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_ENGINE")),
        Some(&"opencode".into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_MODE")),
        Some(&"orchestrator".into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_ORCHESTRATOR")),
        Some(&"1".into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_FLEET")),
        Some(&"claude,codex,opencode,kimi,grok".into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_PROJECT_ROOT")),
        Some(&PathBuf::from(super::support::CWD).into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_ORCH_SESSION")),
        Some(&"orch-fixture".into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_ORCH_PID")),
        Some(&"4242".into())
    );
    assert_eq!(
        command.env.get(OsStr::new("OPENCODE_AUTO_SHARE")),
        Some(&"false".into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_OPENCODE_MODEL")),
        Some(&"local/opencode/custom".into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_CLAUDE_MODEL")),
        Some(&catalog::CLAUDE_DEFAULT_MODEL.into())
    );
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_TOOL_POLICY")),
        Some(&"orchestrator".into())
    );
    assert!(!command.env.contains_key(OsStr::new("OPENAI_API_KEY")));
    assert!(
        command
            .env_remove
            .iter()
            .any(|key| key == OsStr::new("OPENAI_API_KEY"))
    );
    assert!(
        command
            .env_remove
            .iter()
            .any(|key| key == OsStr::new("OPENCODE_API_KEY"))
    );
    assert!(
        command
            .env_remove
            .iter()
            .any(|key| key == OsStr::new("OPENCODE_ZEN_API_KEY"))
    );
    assert!(
        command
            .env_remove
            .iter()
            .any(|key| key == OsStr::new("OPENCODE_AUTH_CONTENT"))
    );
    assert!(
        command
            .env_remove
            .iter()
            .any(|key| key == OsStr::new("XAI_API_KEY"))
    );
}

#[test]
fn orientation_is_the_no_task_instruction_environment_value() {
    let request = request(Engine::Claude).with_orientation("fixture orientation");
    let command =
        build_orchestrator_command(Engine::Claude, OsStr::new("claude"), &request).unwrap();
    assert_eq!(
        command.env.get(OsStr::new(ORCHESTRATOR_INSTRUCTION_ENV)),
        Some(&"fixture orientation".into())
    );
}

#[test]
fn explicit_task_keeps_the_default_instruction_environment_value() {
    let request = request(Engine::Claude)
        .with_orientation("orientation must not rewrite an explicit task")
        .with_initial_task("fixture task");
    let command =
        build_orchestrator_command(Engine::Claude, OsStr::new("claude"), &request).unwrap();
    assert_eq!(
        command.env.get(OsStr::new(ORCHESTRATOR_INSTRUCTION_ENV)),
        Some(&crate::providers::worker::ORCHESTRATOR_DIRECTIVE.into())
    );
}

#[test]
fn profile_isolation_is_injected_for_default_and_non_default_profiles() {
    for engine in Engine::ALL {
        let default_request = request(engine);
        let default_command =
            build_orchestrator_command(engine, OsStr::new(engine.as_str()), &default_request)
                .unwrap();
        let spec = catalog::spec(engine);
        let config = OsStr::new(spec.config_env.as_str());
        if engine == Engine::Codex {
            assert_eq!(
                default_command.env.get(config),
                Some(&PathBuf::from(HOME).join(&spec.default_profile_dir).into())
            );
        } else {
            assert!(
                default_command.env_remove.iter().any(|key| key == config),
                "{engine}"
            );
        }

        let mut isolated = default_request.clone();
        isolated.profile.path = PathBuf::from(HOME)
            .join(".neomax")
            .join(engine.to_string())
            .join("account-2");
        let isolated_command =
            build_orchestrator_command(engine, OsStr::new(engine.as_str()), &isolated).unwrap();
        assert_eq!(
            isolated_command.env.get(config),
            Some(&isolated.profile.path.clone().into()),
            "{engine}"
        );
    }
}

#[test]
fn reserved_profiles_mark_the_orchestrator_environment() {
    let mut request = request(Engine::Claude);
    request.profile.reserved = true;
    let command =
        build_orchestrator_command(Engine::Claude, OsStr::new("claude"), &request).unwrap();
    assert_eq!(
        command.env.get(OsStr::new("NEOMAX_ORCH_RESERVED")),
        Some(&"1".into())
    );
}
