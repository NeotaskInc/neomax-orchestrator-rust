use anyhow::Result;
use neomax_core::orchestration::commands::{Command, Launcher, resolve};

use super::agent_normalize::normalize_command_args;
use super::{authorize_agent_invocation, is_help, is_version, print_help, print_version};
use crate::config;
use crate::context::RuntimeContext;
use crate::error;
use crate::installation;
use crate::launch;
use crate::operations;
use crate::projects;
use crate::queue;
use crate::tasks;

pub fn execute_install(args: &[String]) -> Result<()> {
    error::usage(installation::validate_flags(args))?;
    installation::install_command(args)
}

pub fn execute_uninstall(args: &[String]) -> Result<()> {
    error::usage(installation::validate_flags(args))?;
    installation::uninstall_command(args)
}

pub fn execute(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    authorize_agent_invocation(args)?;
    if is_version(args) {
        print_version(launcher);
        return Ok(());
    }
    if is_help(args) {
        print_help(launcher);
        return Ok(());
    }
    let normalized_args = normalize_command_args(args);
    let args = normalized_args.as_deref().unwrap_or(args);
    if let Some(native_resume_args) = provider_native_resume_args(launcher, args) {
        return launch::run(launcher, &native_resume_args, context);
    }
    if let Some(normalized) = error::usage(operations::normalize_resume(args))? {
        return launch::run(launcher, &normalized, context);
    }
    if matches!(launcher, Launcher::AccountHelper(_)) && !args.iter().any(|arg| arg == "--dry-run")
    {
        return operations::account_helper(launcher, args, context);
    }
    let Some(first) = args.first() else {
        return launch::run(launcher, args, context);
    };
    if first == "account" {
        let operation = args
            .get(1)
            .map(String::as_str)
            .ok_or_else(|| error::usage_error(anyhow::anyhow!("account requires an operation")))?;
        let command = match operation {
            "status" => Command::Status,
            "pause" => Command::Pause,
            "unpause" => Command::Unpause,
            "rotate" => Command::Rotate,
            other => {
                return Err(error::usage_error(anyhow::anyhow!(
                    "unknown account operation: {other}"
                )));
            }
        };
        return operations::execute(launcher, command, &args[2..], context);
    }
    let command = resolve(first).unwrap_or(Command::Dispatch);
    match command {
        Command::Help => {
            print_help(launcher);
            Ok(())
        }
        Command::Config => config::run(context, &args[1..]),
        Command::Solo => {
            let mut solo_args = vec!["--solo".to_owned()];
            solo_args.extend_from_slice(&args[1..]);
            launch::run(launcher, &solo_args, context)
        }
        Command::Select => operations::select(&args[1..], context),
        Command::Why => operations::why(&args[1..], context),
        Command::Projects => projects::list(context, &args[1..]),
        Command::ProjectRegister => projects::register(context, &args[1..]),
        Command::ProjectUnregister => {
            let mut normalized = vec!["--unregister".to_owned()];
            normalized.extend_from_slice(&args[1..]);
            projects::register(context, &normalized)
        }
        Command::Tasks => tasks::run(context, &args[1..]),
        Command::Queue => queue::run(context, &args[1..]),
        Command::Orient
        | Command::UsageWatch
        | Command::Keepalive
        | Command::TurnHook
        | Command::ModelGuard
        | Command::UsageHook
        | Command::Supervise
        | Command::OrchestratorRegister
        | Command::OrchestratorUnregister
        | Command::PickOrchestrator
        | Command::PickNeomax
        | Command::OrchestratorOn
        | Command::Modes
        | Command::SoloSetup
        | Command::Status
        | Command::History
        | Command::Sessions
        | Command::Subagents
        | Command::Orchestrators
        | Command::Usage
        | Command::List
        | Command::Log
        | Command::Diff
        | Command::SubagentDiff
        | Command::Resume
        | Command::Retry
        | Command::Kill
        | Command::Rotate
        | Command::RotateTick
        | Command::SessionRotate
        | Command::SoloRotate
        | Command::RotateAuth
        | Command::Handoff
        | Command::RunAll
        | Command::PullRequest
        | Command::Reconcile
        | Command::Acknowledge
        | Command::Audit
        | Command::Find
        | Command::Premerge
        | Command::Shepherd
        | Command::Issue
        | Command::CiSync
        | Command::Clean
        | Command::Tidy
        | Command::Pause
        | Command::Unpause
        | Command::Paused
        | Command::Portal => operations::execute(launcher, command, &args[1..], context),
        Command::Install => {
            error::usage(installation::validate_flags(&args[1..]))?;
            installation::install_command(&args[1..])
        }
        Command::Uninstall => {
            error::usage(installation::validate_flags(&args[1..]))?;
            installation::uninstall_command(&args[1..])
        }
        Command::Dispatch => {
            if first == "dispatch" {
                let mut dispatch_args = vec!["--worker-dispatch".to_owned()];
                dispatch_args.extend_from_slice(&args[1..]);
                launch::run(launcher, &dispatch_args, context)
            } else if first == "delegate" && !matches!(launcher, Launcher::AccountHelper(_)) {
                // Keep the retired spelling parseable for existing sessions,
                // but preserve its historical guarded-worker semantics on
                // both the universal and provider-pinned launchers.
                let mut dispatch_args = vec!["--worker-dispatch".to_owned()];
                dispatch_args.extend_from_slice(&args[1..]);
                launch::run(launcher, &dispatch_args, context)
            } else {
                launch::run(launcher, args, context)
            }
        }
    }
}

fn provider_native_resume_args(launcher: Launcher, args: &[String]) -> Option<Vec<String>> {
    if !matches!(launcher, Launcher::ProviderOrchestrator(_)) {
        return None;
    }
    let mut command = None;
    let mut index = 0;
    let mut account_seen = false;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "resume" || arg == "--resume" {
            command = Some(index);
            break;
        }
        if arg == "--" {
            return None;
        }
        if !arg.starts_with('-') {
            if !account_seen && arg.parse::<u32>().is_ok() {
                account_seen = true;
                index += 1;
                continue;
            }
            return None;
        }
        let flag = arg.split_once('=').map_or(arg, |(flag, _)| flag);
        if resume_value_flag(flag) && !arg.contains('=') {
            index = index.saturating_add(2);
        } else {
            index += 1;
        }
    }
    let command = command?;
    let mut native = args.to_vec();
    if native[command] == "resume" {
        native[command] = "--resume".into();
    }
    Some(native)
}

fn resume_value_flag(flag: &str) -> bool {
    matches!(
        flag,
        "--engine"
            | "--workers"
            | "--model"
            | "--claude-model"
            | "--codex-model"
            | "-cm"
            | "--opencode-model"
            | "--kimi-model"
            | "--grok-model"
            | "--goal"
            | "--base"
            | "--run-id"
            | "--tag"
            | "--session-id"
            | "--max-turns"
            | "--prefer"
            | "--priority"
            | "--account"
            | "-e"
            | "-t"
            | "-s"
    )
}

#[cfg(test)]
mod tests {
    use super::provider_native_resume_args;
    use neomax_core::Engine;
    use neomax_core::orchestration::commands::Launcher;

    #[test]
    fn pinned_provider_resume_command_becomes_a_native_launch() {
        let args = vec!["resume".into(), "session-42".into()];
        assert_eq!(
            provider_native_resume_args(Launcher::ProviderOrchestrator(Engine::Kimi), &args),
            Some(vec!["--resume".into(), "session-42".into()])
        );
    }

    #[test]
    fn resume_options_may_precede_the_provider_command() {
        let args = vec![
            "--model".into(),
            "fixture/model".into(),
            "--foreground".into(),
            "resume".into(),
            "session-42".into(),
        ];
        assert_eq!(
            provider_native_resume_args(Launcher::ProviderOrchestrator(Engine::Codex), &args)
                .expect("native resume args")
                .get(3)
                .map(String::as_str),
            Some("--resume")
        );
    }

    #[test]
    fn universal_resume_remains_the_managed_lifecycle_command() {
        let args = vec!["resume".into(), "run-42".into()];
        assert!(provider_native_resume_args(Launcher::Universal, &args).is_none());
    }
}
