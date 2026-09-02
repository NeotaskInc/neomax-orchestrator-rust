use std::process::Command;

use super::super::scrub_provider_environment;

const HANDOFF_MARKERS: &[&str] = &[
    "NEOMAX_BIN",
    "NEOMAX_TOOL_MANIFEST",
    "NEOMAX_TOOL_INSTRUCTION",
    "NEOMAX_TOOL_POLICY",
    "NEOMAX_ALLOW_FULL_TOOL_POLICY",
    "NEOMAX_TOOL_DEPTH",
    "NEOMAX_TOOL_MAX_DEPTH",
    "NEOMAX_ROLE",
    "NEOMAX_WORKER",
    "NEOMAX_ORCHESTRATOR",
    "NEOMAX_ENGINE",
    "NEOMAX_MODE",
    "NEOMAX_WORKERS",
    "NEOMAX_PROJECT_ROOT",
    "NEOMAX_ORCHESTRATOR_INSTRUCTION",
    "NEOMAX_ORCHESTRATOR_ORIENTATION",
    "NEOMAX_INVOKED_AS",
    "NEOMAX_ACCOUNT",
    "NEOMAX_ORCH_RESERVED",
    "NEOMAX_ORCH_SESSION",
    "NEOMAX_ORCH_PID",
];

#[test]
fn scrubbed_launch_environment_does_not_copy_profile_or_role_state() {
    let mut command = Command::new("true");
    command
        .env("CLAUDE_CONFIG_DIR", "/profiles/source")
        .env("NEOMAX_PROFILES", "/profiles/source")
        .env("ANTHROPIC_API_KEY", "source-secret")
        .env("NEOMAX_ROLE", "claude")
        .env("NEOMAX_ORCHESTRATOR", "1")
        .env("NEOMAX_ENGINE", "claude")
        .env("NEOMAX_MODE", "orchestrator")
        .env("NEOMAX_WORKERS", "claude,opencode")
        .env("NEOMAX_PROJECT_ROOT", "/source/project")
        .env("NEOMAX_ORCHESTRATOR_INSTRUCTION", "source-directive");
    scrub_provider_environment(&mut command);
    for key in [
        "CLAUDE_CONFIG_DIR",
        "NEOMAX_PROFILES",
        "ANTHROPIC_API_KEY",
        "NEOMAX_ROLE",
        "NEOMAX_ORCHESTRATOR",
        "NEOMAX_ENGINE",
        "NEOMAX_MODE",
        "NEOMAX_WORKERS",
        "NEOMAX_PROJECT_ROOT",
        "NEOMAX_ORCHESTRATOR_INSTRUCTION",
    ] {
        assert!(
            !command
                .get_envs()
                .any(|(name, value)| { name == std::ffi::OsStr::new(key) && value.is_some() })
        );
    }
}

#[test]
fn every_handoff_marker_is_explicitly_removed_before_child_environment_is_built() {
    let mut command = Command::new("true");
    for key in HANDOFF_MARKERS {
        command.env(key, format!("inherited-{key}"));
    }

    scrub_provider_environment(&mut command);

    for key in HANDOFF_MARKERS {
        let marker = std::ffi::OsStr::new(key);
        assert!(
            command
                .get_envs()
                .any(|(name, value)| name == marker && value.is_none()),
            "handoff marker {key} must be removed instead of inherited"
        );
    }
}
