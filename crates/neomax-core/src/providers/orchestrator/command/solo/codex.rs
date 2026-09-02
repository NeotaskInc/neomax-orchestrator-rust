use crate::providers::ProviderCommand;

use super::super::super::types::OrchestratorRequest;

pub(crate) fn build(
    mut command: ProviderCommand,
    request: &OrchestratorRequest,
    model: &str,
) -> ProviderCommand {
    command = command
        .arg("-m")
        .arg(model)
        .arg("-c")
        .arg(format!(
            "model_reasoning_effort={}",
            request.effort.as_deref().unwrap_or("xhigh")
        ))
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
    } else if let Some(task) = request.initial_task.as_deref() {
        command = command.arg(task);
    }
    command
}
