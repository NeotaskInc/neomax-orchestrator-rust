use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use neomax_core::orchestration::handoff::LaunchPlan;
use neomax_core::providers::scrub_provider_environment as scrub_provider_credentials;
use neomax_core::runtime;

pub(super) fn launch_process(plan: &LaunchPlan) -> Result<Option<u32>> {
    let mut command = runtime::process_command(&plan.launcher, &plan.args, &plan.cwd)?;
    command.current_dir(&plan.cwd);
    scrub_provider_environment(&mut command);
    command.envs(&plan.environment);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = command
        .spawn()
        .with_context(|| format!("could not launch {}", plan.launcher))?;
    Ok(Some(child.id()))
}

pub(crate) fn scrub_provider_environment(command: &mut Command) {
    scrub_provider_credentials(command);
    for key in [
        "NEOMAX_ROLE",
        "NEOMAX_WORKER",
        "NEOMAX_ORCHESTRATOR",
        "NEOMAX_ENGINE",
        "NEOMAX_MODE",
        "NEOMAX_WORKERS",
        "NEOMAX_PROJECT_ROOT",
        "NEOMAX_ORCHESTRATOR_INSTRUCTION",
        "NEOMAX_ORCHESTRATOR_ORIENTATION",
        "NEOMAX_BIN",
        "NEOMAX_TOOL_POLICY",
        "NEOMAX_ALLOW_FULL_TOOL_POLICY",
        "NEOMAX_TOOL_MANIFEST",
        "NEOMAX_TOOL_INSTRUCTION",
        "NEOMAX_TOOL_DEPTH",
        "NEOMAX_TOOL_MAX_DEPTH",
        "NEOMAX_INVOKED_AS",
        "NEOMAX_ACCOUNT",
        "NEOMAX_ORCH_RESERVED",
        "NEOMAX_ORCH_SESSION",
        "NEOMAX_ORCH_PID",
    ] {
        command.env_remove(key);
    }
}
