use crate::Result;
use crate::providers::ProviderCommand;
use crate::providers::opencode_policy;

use super::super::super::types::OrchestratorRequest;

pub(crate) fn build(
    mut command: ProviderCommand,
    request: &OrchestratorRequest,
    model: &str,
) -> Result<ProviderCommand> {
    let policy = opencode_policy::content(model)?;
    command = command
        .arg(request.cwd.as_os_str())
        .arg("--model")
        .arg(model)
        .arg("--agent")
        .arg("build")
        .arg("--auto")
        .env("OPENCODE_CONFIG_CONTENT", policy)
        .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
        .env("OPENCODE_DISABLE_SHARE", "1")
        .env("OPENCODE_AUTO_SHARE", "false")
        .env("NO_COLOR", "1")
        .env("GIT_OPTIONAL_LOCKS", "0");
    if request.resume {
        command = command
            .arg("--session")
            .arg(request.session_id.as_deref().unwrap_or_default());
    } else if let Some(task) = request.initial_task.as_deref() {
        command = command.arg(task);
    }
    Ok(command)
}
