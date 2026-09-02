use crate::providers::ProviderCommand;

use super::super::super::types::{OrchestratorRequest, kimi_agent_file};

pub(crate) fn build(
    mut command: ProviderCommand,
    request: &OrchestratorRequest,
    model: &str,
) -> ProviderCommand {
    command = command
        .arg("-m")
        .arg(model)
        .arg("--auto")
        .env("NO_COLOR", "1")
        .env("GIT_OPTIONAL_LOCKS", "0");
    if request.resume {
        command = command
            .arg("-S")
            .arg(request.session_id.as_deref().unwrap_or_default());
    } else {
        let agent_file = request
            .agent_file
            .clone()
            .unwrap_or_else(|| kimi_agent_file(&request.profile.path));
        command = command.arg("--agent-file").arg(agent_file.as_os_str());
    }
    command
}

pub(crate) fn bootstrap(
    command: ProviderCommand,
    request: &OrchestratorRequest,
    model: &str,
) -> ProviderCommand {
    command
        .arg("-m")
        .arg(model)
        .arg("--output-format")
        .arg("stream-json")
        .arg("--prompt")
        .arg(request.initial_task.as_deref().unwrap_or_default())
        .env("NO_COLOR", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
}
