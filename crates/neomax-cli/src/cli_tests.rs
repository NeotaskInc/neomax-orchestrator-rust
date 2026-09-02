use super::*;
use neomax_core::agent_tools::CANONICAL_COMMANDS;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[test]
fn agent_command_resolution_keeps_global_options_out_of_the_command_surface() {
    assert_eq!(
        resolved_agent_command(&args(&["--json", "status"])).unwrap(),
        "status"
    );
    assert_eq!(
        resolved_agent_command(&args(&["--dry-run", "--engine", "opencode", "tasks"])).unwrap(),
        "tasks"
    );
    assert_eq!(
        resolved_agent_command(&args(&["config", "--json", "set", "max-subagents", "4"])).unwrap(),
        "config set"
    );
}

#[test]
fn agent_command_resolution_rejects_empty_and_free_form_tasks() {
    for arguments in [args(&[]), args(&["fix", "the", "build"])] {
        let error = resolved_agent_command(&arguments).unwrap_err().to_string();
        assert!(
            error.contains("agent invocation") || error.contains("agent command"),
            "error was not actionable: {error}"
        );
    }
}

#[test]
fn agent_authorization_resolves_every_canonical_manifest_command() {
    for command in CANONICAL_COMMANDS {
        let arguments = command
            .command
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(
            resolved_agent_command(&arguments).unwrap(),
            command.command,
            "manifest command is not recognized by the agent command resolver: {}",
            command.command
        );
    }
}
