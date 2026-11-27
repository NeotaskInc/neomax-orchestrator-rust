use crate::providers::ProviderCommand;

use super::super::super::types::OrchestratorRequest;
use super::super::root::antigravity::effort;

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
    } else if let Some(task) = request.initial_task.as_deref() {
        command = command.arg("--prompt-interactive").arg(task);
    }
    command
}
