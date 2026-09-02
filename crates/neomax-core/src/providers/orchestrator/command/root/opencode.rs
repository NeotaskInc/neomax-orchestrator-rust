use crate::providers::opencode_policy;
use crate::providers::ProviderCommand;
use crate::Result;

use super::super::super::types::OrchestratorRequest;
use super::super::instructions::orchestrator_prompt;

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
    }
    if !request.resume || request.initial_task.is_some() {
        command = command.arg("--prompt").arg(orchestrator_prompt(request));
    }
    Ok(command)
}
