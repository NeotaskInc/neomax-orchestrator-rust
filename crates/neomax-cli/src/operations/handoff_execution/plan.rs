use anyhow::Result;
use neomax_core::orchestration::handoff::{LaunchPlan, build_launch_plan};

use super::super::options::HandoffOptions;
use super::super::selection::HandoffSelection;
use super::types::HandoffExecution;

pub(crate) fn build_plan(
    options: &HandoffOptions,
    selection: &HandoffSelection,
) -> Result<LaunchPlan> {
    let target = selection
        .target
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no eligible handoff target is available"))?;
    let mut launch_options =
        options.launch_options(&selection.source.account, &target.account.account);
    launch_options.environment.values.insert(
        neomax_core::providers::catalog::spec(target.account.engine).config_env,
        target.account.profile.to_string_lossy().into_owned(),
    );
    build_launch_plan(&launch_options).map_err(Into::into)
}

pub(crate) fn dry_run(
    options: &HandoffOptions,
    selection: &HandoffSelection,
) -> Result<HandoffExecution> {
    Ok(HandoffExecution {
        plan: build_plan(options, selection)?,
        run_id: None,
        continuation: None,
        launched_pid: None,
    })
}
