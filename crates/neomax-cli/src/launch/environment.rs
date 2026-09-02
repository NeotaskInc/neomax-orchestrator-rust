use neomax_core::Engine;
use neomax_core::agent_tools::{
    NEOMAX_BIN_ENV, NEOMAX_TOOL_DEPTH_ENV, NEOMAX_TOOL_INSTRUCTION_ENV, NEOMAX_TOOL_MANIFEST_ENV,
    NEOMAX_TOOL_MAX_DEPTH_ENV,
};

use crate::context::RuntimeContext;

use super::EnvironmentPlan;

pub(crate) fn environment_plan(
    context: &RuntimeContext,
    engine: Engine,
    role: &str,
    model: Option<&str>,
) -> EnvironmentPlan {
    if role == "solo" {
        let mut variables = std::collections::BTreeMap::new();
        variables.insert("NEOMAX_MODE".into(), "solo".into());
        variables.insert(
            "NEOMAX_MODEL".into(),
            model.unwrap_or("selected model").to_owned(),
        );
        return EnvironmentPlan {
            source: "neomax_core::providers::orchestrator".into(),
            role: role.into(),
            policy: "none".into(),
            variables,
        };
    }
    let mut variables = context.settings.agent_environment();
    let policy = if role == "orchestrator" {
        "orchestrator"
    } else {
        "worker"
    };
    variables.extend([
        (NEOMAX_BIN_ENV.into(), "core-managed".into()),
        (NEOMAX_TOOL_MANIFEST_ENV.into(), "core-managed".into()),
        (
            NEOMAX_TOOL_DEPTH_ENV.into(),
            "core-managed recursion depth".into(),
        ),
        (
            NEOMAX_TOOL_MAX_DEPTH_ENV.into(),
            "core-managed recursion limit".into(),
        ),
        (
            NEOMAX_TOOL_INSTRUCTION_ENV.into(),
            "core-managed manifest instruction".into(),
        ),
        (
            "NEOMAX_TOOL_POLICY".into(),
            format!("core-managed {policy} policy"),
        ),
        ("NEOMAX_ROLE".into(), engine.to_string()),
        (
            "PATH".into(),
            "core-managed executable and install-bin precedence".into(),
        ),
    ]);
    EnvironmentPlan {
        source: "neomax_core::agent_tools".into(),
        role: role.into(),
        policy: policy.into(),
        variables,
    }
}
