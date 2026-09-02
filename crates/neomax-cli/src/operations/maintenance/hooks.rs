use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read};

use anyhow::Result;
use neomax_core::orchestration::registry::OrchestratorStore;
use neomax_core::runs::{RunStatus, RunStore};
use serde_json::{Value, json};

use crate::context::RuntimeContext;
use crate::output;

const MAX_HOOK_INPUT_BYTES: u64 = 128 * 1024;

pub(super) fn turn_hook(context: &RuntimeContext) -> Result<()> {
    let environment = env::vars_os().collect::<BTreeMap<_, _>>();
    turn_hook_with_environment(context, &environment)
}

fn turn_hook_with_environment(
    context: &RuntimeContext,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<()> {
    if !is_interactive_orchestrator(environment) {
        return Ok(());
    }
    let _ = read_hook_input();
    let context_line = fleet_context(context);
    output::json(&hook_output("UserPromptSubmit", &context_line))
}

pub(super) fn model_guard(context: &RuntimeContext) -> Result<()> {
    let environment = env::vars_os().collect::<BTreeMap<_, _>>();
    model_guard_with_environment(context, &environment)
}

fn model_guard_with_environment(
    context: &RuntimeContext,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<()> {
    turn_hook_with_environment(context, environment)
}

pub(super) fn usage_hook(context: &RuntimeContext) -> Result<()> {
    let environment = env::vars_os().collect::<BTreeMap<_, _>>();
    usage_hook_with_environment(context, &environment)
}

fn usage_hook_with_environment(
    context: &RuntimeContext,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<()> {
    if !is_interactive_orchestrator(environment) {
        return Ok(());
    }
    let payload = read_hook_input()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .unwrap_or_default();
    let session = environment
        .get(OsStr::new("NEOMAX_ORCH_SESSION"))
        .and_then(|value| value.to_str())
        .map(str::to_owned)
        .or_else(|| {
            payload
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    if let Some(session) = session.filter(|value| !value.trim().is_empty()) {
        let store = OrchestratorStore::new(&context.paths.orchestrators);
        let _ = store.heartbeat(&session, context.now);
    }
    Ok(())
}

fn read_hook_input() -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(MAX_HOOK_INPUT_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

fn is_interactive_orchestrator(environment: &BTreeMap<OsString, OsString>) -> bool {
    environment.contains_key(OsStr::new("NEOMAX_ROLE"))
        && !environment.contains_key(OsStr::new("NEOMAX_WORKER"))
        && environment
            .get(OsStr::new("NEOMAX_MODE"))
            .map(OsString::as_os_str)
            != Some(OsStr::new("solo"))
}

fn fleet_context(context: &RuntimeContext) -> String {
    let runs = RunStore::new(&context.paths.runs);
    let active = runs
        .all()
        .unwrap_or_default()
        .into_iter()
        .filter(|run| matches!(run.status, RunStatus::Running | RunStatus::Orphaned))
        .collect::<Vec<_>>();
    let accounts = active
        .iter()
        .map(|run| run.profile.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    format!(
        "Neomax orchestrator mode: {} worker(s) running across {} account(s). `neomax status` shows live routing and `neomax help` shows the command registry.",
        active.len(),
        accounts
    )
}

fn hook_output(event: &str, context: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": event,
            "additionalContext": context,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn non_orchestrator_hooks_are_silent() {
        let fixture = fixture();
        let environment = BTreeMap::new();
        turn_hook_with_environment(&fixture.context, &environment).unwrap();
        model_guard_with_environment(&fixture.context, &environment).unwrap();
    }

    #[test]
    fn usage_hook_accepts_malformed_input_and_does_not_fail() {
        let fixture = fixture();
        let environment = BTreeMap::new();
        usage_hook_with_environment(&fixture.context, &environment).unwrap();
    }
}
