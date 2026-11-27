use crate::providers::ProviderCommand;
use crate::providers::catalog::ANTIGRAVITY_DEFAULT_EFFORT;

use super::super::super::types::OrchestratorRequest;
use super::super::instructions::orchestrator_prompt;

pub(crate) fn build(
    mut command: ProviderCommand,
    request: &OrchestratorRequest,
    model: &str,
) -> ProviderCommand {
    command = command
        .arg("--model")
        .arg(model)
        .arg("--effort")
        .arg(effort(request))
        .arg("--dangerously-skip-permissions")
        .env("NO_COLOR", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env(crate::providers::antigravity::AUTO_UPDATE_ENV, "true");
    if request.resume {
        command = command
            .arg("--conversation")
            .arg(request.session_id.as_deref().unwrap_or_default());
    }
    if !request.resume || request.initial_task.is_some() {
        command = command
            .arg("--prompt-interactive")
            .arg(orchestrator_prompt(request));
    }
    command
}

pub(crate) fn effort(request: &OrchestratorRequest) -> &str {
    request
        .effort
        .as_deref()
        .or_else(|| request.ultra.then_some("xhigh"))
        .unwrap_or(ANTIGRAVITY_DEFAULT_EFFORT)
}
