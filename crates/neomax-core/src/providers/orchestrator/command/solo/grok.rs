use crate::providers::ProviderCommand;

use super::super::super::types::OrchestratorRequest;

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
        .arg("--cwd")
        .arg(request.cwd.as_os_str())
        .env("NO_COLOR", "1")
        .env("GIT_OPTIONAL_LOCKS", "0");
    if request.resume {
        command = command
            .arg("--resume")
            .arg(request.session_id.as_deref().unwrap_or_default());
    } else if let Some(task) = request.initial_task.as_deref() {
        command = command.arg(task);
    }
    command
}
