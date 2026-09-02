mod account_helpers;
mod handoff;
mod maintenance;
mod orchestrators;
mod plans;
mod portal;
mod rotation;
pub(crate) mod run_lifecycle;
mod sessions;
mod status;
mod usage;
mod workflows;

pub(crate) use orchestrators::setup_profile;
pub(crate) use rotation::arm_profile;

use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::orchestration::commands::{Command, Launcher};

use crate::context::RuntimeContext;
use crate::error;

pub(crate) fn no_task_orientation(
    launcher: Launcher,
    engine: Engine,
    scope: &neomax_core::WorkerScope,
    worker_models: &std::collections::BTreeMap<Engine, String>,
    context: &RuntimeContext,
) -> Result<String> {
    maintenance::no_task_orientation(launcher, engine, scope, worker_models, context)
}

pub(crate) fn exit_code(error: &anyhow::Error) -> Option<i32> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<handoff::HandoffExitCode>())
        .map(|value| value.0)
        .or_else(|| error::exit_code(error))
}

pub(crate) fn normalize_resume(args: &[String]) -> Result<Option<Vec<String>>> {
    Ok(sessions::normalize_resume(args)?.map(|launch| launch.args))
}

pub(crate) fn resolve_resume_target(
    context: &RuntimeContext,
    selector: Option<&str>,
) -> Result<sessions::ResumeTarget> {
    sessions::resolve_target(context, selector)
}

pub(crate) fn resolve_resume_target_for_engine(
    context: &RuntimeContext,
    engine: Engine,
    selector: Option<&str>,
) -> Result<sessions::ResumeTarget> {
    sessions::resolve_target_for_engine(context, engine, selector)
}

pub(crate) fn select(args: &[String], context: &RuntimeContext) -> Result<()> {
    orchestrators::select(args, context)
}

pub(crate) fn why(args: &[String], context: &RuntimeContext) -> Result<()> {
    orchestrators::why(args, context)
}

pub(crate) fn account_helper(
    launcher: Launcher,
    args: &[String],
    context: &RuntimeContext,
) -> Result<()> {
    account_helpers::run(launcher, args, context)
}

pub(crate) fn execute(
    launcher: Launcher,
    command: Command,
    args: &[String],
    context: &RuntimeContext,
) -> Result<()> {
    match command {
        Command::Orient
        | Command::UsageWatch
        | Command::Keepalive
        | Command::TurnHook
        | Command::ModelGuard
        | Command::UsageHook
        | Command::Supervise => maintenance::execute(launcher, command, args, context),
        Command::OrchestratorRegister
        | Command::OrchestratorUnregister
        | Command::PickOrchestrator
        | Command::PickNeomax
        | Command::OrchestratorOn
        | Command::Modes
        | Command::SoloSetup => orchestrators::execute(launcher, command, args, context),
        Command::Pause | Command::Unpause | Command::Paused => {
            orchestrators::execute(launcher, command, args, context)
        }
        Command::List
        | Command::Log
        | Command::Resume
        | Command::Retry
        | Command::Kill
        | Command::Diff
        | Command::SubagentDiff => {
            let lifecycle = run_lifecycle::RunLifecycleCommand::from_core(command)
                .ok_or_else(|| anyhow::anyhow!("unsupported run lifecycle command"))?;
            run_lifecycle::execute_native(lifecycle, context, args)
        }
        Command::Handoff => handoff::run(launcher, context, args),
        Command::Status => status::run(context, args),
        Command::History => {
            let lifecycle = run_lifecycle::RunLifecycleCommand::from_core(command)
                .ok_or_else(|| anyhow::anyhow!("unsupported run lifecycle command"))?;
            run_lifecycle::execute_native(lifecycle, context, args)
        }
        Command::Sessions => sessions::run_sessions(context, args),
        Command::Subagents => sessions::run_subagents(context, args),
        Command::Orchestrators => status::orchestrators(context, args),
        Command::Usage => usage::run(context, args),
        Command::Portal => portal::run(args),
        Command::Rotate
        | Command::RotateTick
        | Command::SessionRotate
        | Command::SoloRotate
        | Command::RotateAuth => rotation::execute(launcher, command, args, context),
        Command::RunAll => plans::execute_command(args, context),
        Command::PullRequest
        | Command::Reconcile
        | Command::Acknowledge
        | Command::Audit
        | Command::Find
        | Command::Premerge
        | Command::Shepherd
        | Command::Issue
        | Command::CiSync
        | Command::Clean
        | Command::Tidy => workflows::execute(command, args, context),
        _ => bail!("command {command:?} is not owned by the local status/usage adapters"),
    }
}
