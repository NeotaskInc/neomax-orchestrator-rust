#[path = "rotation/execution.rs"]
mod execution;
#[path = "rotation/options.rs"]
mod options;
#[path = "rotation/report.rs"]
mod report;

use anyhow::Result;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::continuation::RotationTrigger;

use crate::context::RuntimeContext;

pub(crate) use report::RotationReport;

pub(crate) fn rotate(
    launcher: Launcher,
    context: &RuntimeContext,
    args: &[String],
    trigger: RotationTrigger,
) -> Result<Vec<RotationReport>> {
    execution::resume(launcher, context, args, trigger)
}

pub(crate) fn rotate_model_free(
    launcher: Launcher,
    context: &RuntimeContext,
    args: &[String],
    trigger: RotationTrigger,
) -> Result<Vec<RotationReport>> {
    execution::model_free(launcher, context, args, trigger)
}

#[cfg(test)]
pub(crate) use options::RotationOptions;

#[cfg(test)]
#[path = "rotation/tests.rs"]
mod tests;
