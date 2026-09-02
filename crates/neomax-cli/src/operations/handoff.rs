#[path = "handoff_execution.rs"]
mod execution;
#[path = "handoff_options.rs"]
mod options;
#[path = "handoff_selection.rs"]
mod selection;

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::Path;

use anyhow::{Result, bail};
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::continuation::RotationTrigger;
use neomax_core::orchestration::registry::OrchestratorRecord;
use serde_json::json;

use self::execution::{HandoffExecution, dry_run, execute, snapshots};
use self::options::{HandoffOptions, parse};
use self::selection::{HandoffSelection, select};
use crate::context::RuntimeContext;
use crate::error;
use crate::output;
use crate::parser;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HandoffExitCode(pub(crate) i32);

impl Display for HandoffExitCode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "handoff advisory exit {}", self.0)
    }
}

impl std::error::Error for HandoffExitCode {}

pub(crate) fn run(launcher: Launcher, context: &RuntimeContext, args: &[String]) -> Result<()> {
    let options = error::usage(parse(launcher, context, args))?;
    let runtime = context.provider_runtime()?;
    let accounts = snapshots(context, &runtime)?;
    let selection = select(&options, context, &accounts)?;
    run_selected(options, selection, context, RotationTrigger::Manual)
}

pub(crate) fn run_untracked_with_trigger(
    launcher: Launcher,
    context: &RuntimeContext,
    args: &[String],
    source_profile: &Path,
    trigger: RotationTrigger,
) -> Result<HandoffExecution> {
    let mut options = error::usage(parse(launcher, context, args))?;
    let runtime = context.provider_runtime()?;
    let accounts = snapshots(context, &runtime)?;
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let selection = selection::select_with_profile(
        &options,
        context,
        &accounts,
        source_profile.to_path_buf(),
        &environment,
    )?;
    if selection.target.is_none() {
        bail!(
            "no other logged-in {} account has quota headroom to hand off to",
            selection.engine
        );
    }
    if !options.reason_explicit {
        options.reason = selection.check.advice.reason.clone();
    }
    execute(&options, &selection, context, trigger)
}

pub(crate) fn run_live(
    launcher: Launcher,
    context: &RuntimeContext,
    args: &[String],
    record: &OrchestratorRecord,
    quota_only: bool,
) -> Result<()> {
    let trigger = if quota_only {
        RotationTrigger::Tick
    } else {
        RotationTrigger::Manual
    };
    run_live_with_trigger(launcher, context, args, record, trigger, quota_only)
}

pub(crate) fn run_live_with_trigger(
    launcher: Launcher,
    context: &RuntimeContext,
    args: &[String],
    record: &OrchestratorRecord,
    trigger: RotationTrigger,
    quota_only: bool,
) -> Result<()> {
    let mut live_args = args.to_vec();
    if parser::value(&live_args, "--engine")?.is_none() {
        live_args.extend(["--engine".into(), record.engine.to_string()]);
    }
    if parser::value(&live_args, "--base")?.is_none() && !record.cwd.as_os_str().is_empty() {
        live_args.extend(["--base".into(), record.cwd.to_string_lossy().into_owned()]);
    }
    if parser::value(&live_args, "--session")?.is_none()
        && parser::value(&live_args, "--session-id")?.is_none()
    {
        live_args.extend(["--session".into(), record.session.clone()]);
    }
    let options = error::usage(parse(launcher, context, &live_args))?;
    let runtime = context.provider_runtime()?;
    let accounts = snapshots(context, &runtime)?;
    let (options, selection) =
        selection::select_live_orchestrator(&options, context, &accounts, record)?;
    if quota_only && !selection.check.advice.advised {
        if options.json {
            return output::json(&json!({
                "command": "rotate-tick",
                "selection": "interactive-orchestrator",
                "session": record.session,
                "engine": record.engine.to_string(),
                "rotated": [],
                "reason": "interactive source is below the quota rotation wall"
            }));
        }
        return Ok(());
    }
    run_selected(options, selection, context, trigger)
}

fn run_selected(
    mut options: HandoffOptions,
    selection: HandoffSelection,
    context: &RuntimeContext,
    trigger: RotationTrigger,
) -> Result<()> {
    if options.check {
        let reason_override = options.reason_explicit.then_some(options.reason.as_str());
        render_check(&selection, options.json, reason_override)?;
        let exit_code = selection.check.exit_code();
        if exit_code != 0 {
            return Err(anyhow::Error::new(HandoffExitCode(exit_code)));
        }
        return Ok(());
    }
    if selection.target.is_none() {
        bail!(
            "no other logged-in {} account has quota headroom to hand off to",
            selection.engine
        );
    }

    if !options.reason_explicit {
        options.reason = selection.check.advice.reason.clone();
    }

    let result = if options.dry_run {
        dry_run(&options, &selection)?
    } else {
        execute(&options, &selection, context, trigger)?
    };
    render_handoff(&options, &selection, &result)
}

fn render_check(
    selection: &HandoffSelection,
    json_output: bool,
    reason_override: Option<&str>,
) -> Result<()> {
    let check = &selection.check;
    let reason = reason_override.unwrap_or(&check.advice.reason);
    if json_output {
        return output::json(&json!({
            "engine": check.engine.to_string(),
            "account": check.account,
            "five_hour": round_percent(check.five_hour),
            "seven_day": round_percent(check.seven_day),
            "advised": check.advice.advised,
            "reason": reason,
            "target_account": check.target_account,
            "target_weekly_resets": check.target_weekly_resets,
            "target_email": check.target_email,
        }));
    }
    println!(
        "orchestrator = {} account {} | 5h {:.0}% | 7d {:.0}% => {}",
        check.engine,
        check.account,
        check.five_hour,
        check.seven_day,
        if check.advice.advised {
            format!("ROTATE ADVISED ({reason})")
        } else {
            "ok".into()
        }
    );
    if check.advice.advised {
        if let Some(target) = check.target_account.as_deref() {
            println!(
                "  -> hand off to account {} (soonest weekly reset{}). Run: neomax handoff",
                target,
                check
                    .target_weekly_resets
                    .as_deref()
                    .map_or_else(String::new, |value| format!(", {value}"))
            );
        } else {
            println!(
                "  -> no fresher {} account available - lean on the other worker pools / pause.",
                selection.engine
            );
        }
    }
    Ok(())
}

fn render_handoff(
    options: &HandoffOptions,
    selection: &HandoffSelection,
    result: &HandoffExecution,
) -> Result<()> {
    let target = selection
        .target
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no eligible handoff target is available"))?;
    if options.json {
        return output::json(&json!({
            "source_account": selection.source.account,
            "target_account": target.account.account,
            "reason": options.reason,
            "source_profile": selection.current_profile,
            "target_profile": target.account.profile,
            "selection_tier": format!("{:?}", target.tier),
            "run_id": result.run_id,
            "continuation": result.continuation.map(|mode| format!("{mode:?}")),
            "launched_pid": result.launched_pid,
            "plan": {
                "engine": result.plan.engine.to_string(),
                "launcher": result.plan.launcher,
                "args": result.plan.args,
                "cwd": result.plan.cwd,
                "environment": result.plan.environment,
                "shell_command": result.plan.shell_command,
                "headless": result.plan.headless,
            },
        }));
    }
    if options.dry_run {
        println!(
            "DRY-RUN handoff: {} account {} ({}) -> account {}",
            selection.engine, selection.source.account, options.reason, target.account.account
        );
        println!("  would launch: {}", result.plan.shell_command);
        return Ok(());
    }
    println!(
        "neomax: ROTATED orchestrator -> {} account {} ({}). Reason: {}.",
        selection.engine,
        target.account.account,
        target.account.profile.display(),
        options.reason
    );
    if let Some(run_id) = result.run_id.as_deref() {
        println!("  continued tracked run {run_id} with durable work state preserved.");
    }
    if let Some(pid) = result.launched_pid {
        println!(
            "  launched {} (supervisor pid {pid}).",
            result.plan.launcher
        );
    } else {
        println!("  start the new orchestrator with:");
        println!("    {}", result.plan.shell_command);
    }
    Ok(())
}

fn round_percent(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod facade_tests {
    use neomax_core::orchestration::handoff::HandoffCheck;

    use super::*;

    #[test]
    fn advisory_exit_type_keeps_the_reference_exit_code() {
        let error = anyhow::Error::new(HandoffExitCode(HandoffCheck::ROTATE_EXIT));
        assert_eq!(error.downcast_ref::<HandoffExitCode>().unwrap().0, 10);
    }

    #[test]
    fn percentage_renderer_is_stable() {
        assert_eq!(round_percent(99.96), 100.0);
        assert_eq!(round_percent(42.34), 42.3);
    }
}
