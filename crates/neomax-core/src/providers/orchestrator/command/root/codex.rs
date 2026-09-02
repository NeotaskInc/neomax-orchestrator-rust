use crate::providers::ProviderCommand;

use super::super::super::types::OrchestratorRequest;
use super::super::instructions::orchestrator_prompt;

pub(crate) fn build(
    mut command: ProviderCommand,
    request: &OrchestratorRequest,
    model: &str,
) -> ProviderCommand {
    command = command.arg("-m").arg(model);
    if let Some(effort) = request
        .effort
        .as_deref()
        .or_else(|| request.ultra.then_some("xhigh"))
    {
        command = command
            .arg("-c")
            .arg(format!("model_reasoning_effort={effort}"));
    }
    command = command
        .arg("-c")
        .arg("service_tier=fast")
        .arg("-a")
        .arg("never")
        .arg("-s")
        .arg("danger-full-access")
        .arg("-C")
        .arg(request.cwd.as_os_str())
        .env("NO_COLOR", "1")
        .env("GIT_OPTIONAL_LOCKS", "0");
    if request.resume {
        command = command
            .arg("resume")
            .arg(request.session_id.as_deref().unwrap_or_default());
    }
    if !request.resume || request.initial_task.is_some() {
        command = command.arg(orchestrator_prompt(request));
    }
    command
}
