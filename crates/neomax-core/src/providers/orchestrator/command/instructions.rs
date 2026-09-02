use crate::providers::worker::ORCHESTRATOR_DIRECTIVE;

use super::super::types::OrchestratorRequest;

pub(crate) fn startup_instruction(request: &OrchestratorRequest) -> &str {
    if request.initial_task.is_none() {
        request.instruction()
    } else {
        ORCHESTRATOR_DIRECTIVE
    }
}

pub(crate) fn orchestrator_prompt(request: &OrchestratorRequest) -> String {
    let mut parts = vec![startup_instruction(request).to_owned()];
    if let Some(goal) = request.goal.as_deref() {
        parts.push(goal_block(goal, request.max_turns));
    }
    if let Some(task) = request.initial_task.as_deref() {
        parts.push(task.to_owned());
    }
    parts.join("\n\n")
}

pub(crate) fn claude_initial_task(request: &OrchestratorRequest) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(goal) = request.goal.as_deref() {
        parts.push(format!("/goal {goal}"));
    }
    if let Some(task) = request.initial_task.as_deref() {
        parts.push(task.to_owned());
    }
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

pub(crate) fn grok_initial_task(request: &OrchestratorRequest) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(goal) = request.goal.as_deref() {
        parts.push(goal_block(goal, request.max_turns));
    }
    if let Some(task) = request.initial_task.as_deref() {
        parts.push(task.to_owned());
    }
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

pub(crate) fn goal_block(goal: &str, max_turns: Option<u32>) -> String {
    let cap = max_turns.map_or_else(String::new, |turns| {
        format!(
            " Make at most {turns} rounds of self-correction; if the objective still is not met, stop and report exactly what remains."
        )
    });
    format!(
        "OBJECTIVE: do not finish until this condition holds:\n{goal}\nWork autonomously toward it, then VERIFY the condition is actually met by running the relevant checks, tests, or builds before you end. If verification fails, keep working until it passes.{cap}"
    )
}
