use crate::providers::ProviderCommand;

use super::super::super::types::OrchestratorRequest;
use super::super::instructions::{claude_initial_task, startup_instruction};

pub(crate) fn build(
    mut command: ProviderCommand,
    request: &OrchestratorRequest,
    model: &str,
) -> ProviderCommand {
    command = command
        .arg("--model")
        .arg(model)
        .arg("--dangerously-skip-permissions")
        .arg("--append-system-prompt")
        .arg(startup_instruction(request));
    if request.ultra {
        command = command
            .arg("--settings")
            .arg(serde_json::json!({"ultracode": true}).to_string());
    }
    if let Some(effort) = request.effort.as_deref() {
        command = command.arg("--effort").arg(effort);
    }
    if let Some(max_turns) = request.max_turns {
        command = command.arg("--max-turns").arg(max_turns.to_string());
    }
    if request.resume {
        command = command
            .arg("--resume")
            .arg(request.session_id.as_deref().unwrap_or_default());
    }
    if let Some(task) = claude_initial_task(request) {
        command = command.arg(task);
    }
    command.remove_env("CLAUDE_CODE_DISABLE_REFUSAL_FALLBACK")
}
