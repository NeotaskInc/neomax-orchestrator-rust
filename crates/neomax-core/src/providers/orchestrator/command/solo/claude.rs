use crate::providers::ProviderCommand;

use super::super::super::types::OrchestratorRequest;

pub(crate) fn build(
    mut command: ProviderCommand,
    request: &OrchestratorRequest,
    model: &str,
) -> ProviderCommand {
    command = command
        .arg("--model")
        .arg(model)
        .arg("--dangerously-skip-permissions")
        .arg("--settings")
        .arg(serde_json::json!({"ultracode": true}).to_string())
        .arg("--effort")
        .arg(request.effort.as_deref().unwrap_or("xhigh"));
    if request.resume {
        command = command
            .arg("--resume")
            .arg(request.session_id.as_deref().unwrap_or_default());
    }
    if let Some(task) = request.initial_task.as_deref() {
        command = command.arg(task);
    }
    command
}
