mod account;
mod claude_profile;
mod coordinator;
mod launch;
mod orchestrator;
mod record;
mod report;
mod rerun;
mod selection;
mod worktree;

#[cfg(test)]
mod tests;

use anyhow::Result;
use neomax_core::orchestration::commands::Launcher;

use crate::context::RuntimeContext;
use crate::launch::types::LaunchOptions;

#[cfg(all(test, unix))]
pub(crate) use launch::run_with_registry;
pub(crate) use rerun::execute_record_with_runtime;

pub(crate) fn run(
    launcher: Launcher,
    options: LaunchOptions,
    context: &RuntimeContext,
    json_output: bool,
) -> Result<()> {
    launch::run(launcher, options, context, json_output)
}
