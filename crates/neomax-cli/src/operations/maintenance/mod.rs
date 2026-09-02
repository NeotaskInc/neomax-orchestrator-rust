mod hooks;
mod keepalive;
mod orient;
mod supervise;
mod usage_watch;

use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::orchestration::commands::{Command, Launcher};

use crate::context::RuntimeContext;

pub(crate) fn no_task_orientation(
    launcher: Launcher,
    engine: Engine,
    scope: &neomax_core::WorkerScope,
    worker_models: &std::collections::BTreeMap<Engine, String>,
    context: &RuntimeContext,
) -> Result<String> {
    orient::no_task_instruction(launcher, engine, scope, worker_models, context)
}

pub(crate) fn execute(
    launcher: Launcher,
    command: Command,
    args: &[String],
    context: &RuntimeContext,
) -> Result<()> {
    match command {
        Command::Orient => orient::run(launcher, args, context),
        Command::UsageWatch => usage_watch::run(args, context),
        Command::Keepalive => keepalive::run(args, context),
        Command::TurnHook => hooks::turn_hook(context),
        Command::ModelGuard => hooks::model_guard(context),
        Command::UsageHook => hooks::usage_hook(context),
        Command::Supervise => supervise::run(args, context),
        _ => bail!("command {command:?} is not owned by maintenance operations"),
    }
}
