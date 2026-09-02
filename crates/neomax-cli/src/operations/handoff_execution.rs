#[path = "handoff_execution/continuation.rs"]
mod continuation;
#[path = "handoff_execution/plan.rs"]
mod plan;
#[path = "handoff_execution/process.rs"]
mod process;
#[path = "handoff_execution/snapshots.rs"]
mod snapshots;
#[path = "handoff_execution/types.rs"]
mod types;

use anyhow::Result;
use neomax_core::orchestration::continuation::RotationTrigger;
use neomax_core::runs::RunStore;

use self::continuation::save_untracked_baton;
use self::process::launch_process;
use super::options::HandoffOptions;
use super::selection::HandoffSelection;
use crate::context::RuntimeContext;

pub(crate) use continuation::{continue_tracked_run, find_current_run};
pub(crate) use plan::{build_plan, dry_run};
pub(crate) use snapshots::snapshots;
pub(crate) use types::HandoffExecution;

pub(crate) fn execute(
    options: &HandoffOptions,
    selection: &HandoffSelection,
    context: &RuntimeContext,
    trigger: RotationTrigger,
) -> Result<HandoffExecution> {
    let plan = build_plan(options, selection)?;
    let target = selection
        .target
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no eligible handoff target is available"))?;
    let runs = RunStore::new(&context.paths.runs);
    let run = find_current_run(&runs, options, selection, context)?;
    let continuation = if let Some(run) = run.as_ref() {
        Some(continue_tracked_run(
            run,
            options,
            selection,
            target.account.clone(),
            context,
            &runs,
            trigger,
        )?)
    } else {
        save_untracked_baton(options, selection, context)?;
        None
    };
    let launched_pid = launch_process(&plan)?;
    if let (Some(run), Some(pid)) = (run.as_ref(), launched_pid) {
        let mut updated = runs.load(&run.id)?;
        updated.supervisor_pid = Some(pid);
        runs.save_preserving_control_markers(&updated)?;
    }
    Ok(HandoffExecution {
        plan,
        run_id: run.as_ref().map(|run| run.id.clone()),
        continuation,
        launched_pid,
    })
}

#[cfg(test)]
pub(crate) use process::scrub_provider_environment;

#[cfg(test)]
#[path = "handoff_execution/tests/mod.rs"]
mod tests;
