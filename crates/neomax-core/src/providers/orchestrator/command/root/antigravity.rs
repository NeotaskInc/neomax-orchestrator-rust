use crate::providers::ProviderCommand;
use crate::providers::catalog::ANTIGRAVITY_DEFAULT_EFFORT;

use crate::orchestration::delegation::ANTIGRAVITY_DELEGATION;

use super::super::super::types::OrchestratorRequest;
use super::super::instructions::{goal_block, startup_instruction};

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
            .arg(initial_prompt(request));
    }
    command
}

fn initial_prompt(request: &OrchestratorRequest) -> String {
    let mut parts = vec![
        startup_instruction(request).to_owned(),
        ANTIGRAVITY_DELEGATION.to_owned(),
    ];
    if let Some(goal) = request.goal.as_deref() {
        parts.push(goal_block(goal, request.max_turns));
    }
    if let Some(task) = request.initial_task.as_deref() {
        parts.push(task.to_owned());
    }
    parts.join("\n\n")
}

pub(crate) fn effort(request: &OrchestratorRequest) -> &str {
    request
        .effort
        .as_deref()
        .or_else(|| request.ultra.then_some("xhigh"))
        .unwrap_or(ANTIGRAVITY_DEFAULT_EFFORT)
}
