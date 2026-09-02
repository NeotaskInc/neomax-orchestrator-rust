use crate::providers::ProviderCommand;

use super::super::super::types::OrchestratorRequest;
use super::super::instructions::{grok_initial_task, startup_instruction};

pub(crate) fn build(
    mut command: ProviderCommand,
    request: &OrchestratorRequest,
    model: &str,
) -> ProviderCommand {
    command = command
        .arg("--no-auto-update")
        .arg("--model")
        .arg(model)
        .arg("--always-approve")
        .arg("--rules")
        .arg(startup_instruction(request))
        .arg("--cwd")
        .arg(request.cwd.as_os_str())
        .env("NO_COLOR", "1")
        .env("GIT_OPTIONAL_LOCKS", "0");
    if let Some(max_turns) = request.max_turns {
        command = command.arg("--max-turns").arg(max_turns.to_string());
    }
    if request.resume {
        command = command
            .arg("--resume")
            .arg(request.session_id.as_deref().unwrap_or_default());
    }
    if let Some(task) = grok_initial_task(request) {
        command = command.arg(task);
    }
    command
}
