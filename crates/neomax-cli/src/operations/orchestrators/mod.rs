mod controls;
mod modes;
mod registry;
mod selection;
mod solo;

pub(crate) use solo::setup_profile;

use anyhow::{Result, bail};
use neomax_core::orchestration::commands::{Command, Launcher};

use crate::context::RuntimeContext;

pub(crate) fn execute(
    launcher: Launcher,
    command: Command,
    args: &[String],
    context: &RuntimeContext,
) -> Result<()> {
    match command {
        Command::OrchestratorRegister | Command::OrchestratorUnregister => {
            registry::execute(launcher, command, args, context)
        }
        Command::PickOrchestrator | Command::PickNeomax | Command::OrchestratorOn => {
            selection::execute(launcher, command, args, context)
        }
        Command::Modes => modes::execute(args, context),
        Command::SoloSetup => solo::execute(launcher, args, context),
        Command::Pause | Command::Unpause | Command::Paused => {
            controls::execute(command, args, context)
        }
        _ => bail!("command {command:?} is not owned by orchestrator operations"),
    }
}

pub(crate) fn select(args: &[String], context: &RuntimeContext) -> Result<()> {
    selection::select(args, context)
}

pub(crate) fn why(args: &[String], context: &RuntimeContext) -> Result<()> {
    selection::why(args, context)
}
