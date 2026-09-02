mod args;
mod lifecycle;
#[cfg(test)]
mod queue;
mod status;
mod types;

use anyhow::Result;
use neomax_core::orchestration::commands::Command;
use neomax_core::providers::scrub_provider_environment;

use crate::context::RuntimeContext;
use crate::error;

pub(crate) use args::PlanAction;
pub(crate) use lifecycle::PlanFactory;

pub(crate) fn execute(action: PlanAction, args: &[String], context: &RuntimeContext) -> Result<()> {
    let parsed = error::usage(args::parse_action(action, args))?;
    error::usage(
        parsed
            .runtime
            .validate_against_settings(&context.settings, None),
    )?;
    match action {
        PlanAction::RunAll => run_all(parsed, context),
        PlanAction::Attach => attach(parsed, context),
        PlanAction::Tick => tick(parsed, context),
        PlanAction::Interrupt => interrupt(parsed, context),
        PlanAction::Recover => recover(parsed, context),
        PlanAction::Status | PlanAction::List => status(parsed, context),
    }
}

pub(crate) fn action_for(command: Command) -> Option<PlanAction> {
    match command {
        Command::RunAll => Some(PlanAction::RunAll),
        _ => None,
    }
}

pub(crate) fn execute_command(args: &[String], context: &RuntimeContext) -> Result<()> {
    let action = match args.first().map(String::as_str) {
        Some("attach") => PlanAction::Attach,
        Some("tick") => PlanAction::Tick,
        Some("interrupt") => PlanAction::Interrupt,
        Some("recover") => PlanAction::Recover,
        Some("status") => PlanAction::Status,
        Some("list") => PlanAction::List,
        _ => action_for(Command::RunAll)
            .ok_or_else(|| anyhow::anyhow!("run-all is not registered"))?,
    };
    let action_args = match action {
        PlanAction::RunAll => args,
        _ => &args[1..],
    };
    execute(action, action_args, context)
}

fn run_all(mut parsed: args::PlanArguments, context: &RuntimeContext) -> Result<()> {
    let path = parsed
        .plan_path
        .clone()
        .ok_or_else(|| anyhow::anyhow!("run-all requires a plan JSON path"))?;
    let factory = lifecycle::ProductionPlanFactory::new(
        context.paths.clone(),
        context.settings.clone(),
        parsed.scope.clone(),
        context.provider_runtime()?,
    );
    parsed.runtime = parsed
        .runtime
        .resolve_run_all(&context.settings, factory.eligible_account_count())?;
    let run_args = args::normalize_run_all(args::RunAllInput {
        path,
        cwd: context.cwd.clone(),
        scope: parsed.scope.clone(),
        runtime: parsed.runtime,
        repository: parsed.repository.clone(),
        base: parsed.base.clone(),
        integration_branch: parsed.integration_branch.clone(),
        plan_id: parsed.plan_id.clone(),
    })?;
    let spec = args::load_run_all(&run_args, &context.cwd, &parsed.scope)?;
    if parsed.wait {
        let report = factory.run_all_with_max_ticks(spec, parsed.runtime.max_ticks)?;
        return render(&report, parsed.json);
    }
    let plan_id = spec.plan_id.clone();
    let mut lifecycle = factory.start(spec)?;
    lifecycle.detach()?;
    if let Err(error) = spawn_scheduler_detached(&parsed, &plan_id) {
        let _ = lifecycle.interrupt(Some(format!(
            "could not start scheduler supervisor: {error}"
        )));
        return Err(error);
    }
    drop(lifecycle);
    render(&status::plan_status(&context.paths, &plan_id)?, parsed.json)
}

fn attach(parsed: args::PlanArguments, context: &RuntimeContext) -> Result<()> {
    let plan_id = required_plan_id(&parsed)?;
    let factory = lifecycle::ProductionPlanFactory::new(
        context.paths.clone(),
        context.settings.clone(),
        parsed.scope.clone(),
        context.provider_runtime()?,
    );
    parsed
        .runtime
        .validate_against_settings(&context.settings, Some(factory.eligible_account_count()))?;
    if parsed.wait {
        let mut lifecycle = factory.attach(&plan_id, parsed.runtime.runtime)?;
        let report = lifecycle.run_until_terminal(parsed.runtime.max_ticks)?;
        return render(&report, parsed.json);
    }
    spawn_scheduler_detached(&parsed, &plan_id)?;
    render(&status::plan_status(&context.paths, &plan_id)?, parsed.json)
}

fn tick(parsed: args::PlanArguments, context: &RuntimeContext) -> Result<()> {
    let plan_id = required_plan_id(&parsed)?;
    let factory = lifecycle::ProductionPlanFactory::new(
        context.paths.clone(),
        context.settings.clone(),
        parsed.scope.clone(),
        context.provider_runtime()?,
    );
    parsed
        .runtime
        .validate_against_settings(&context.settings, Some(factory.eligible_account_count()))?;
    let mut lifecycle = factory.attach(&plan_id, parsed.runtime.runtime)?;
    let report = lifecycle.tick()?;
    if parsed.json {
        return crate::output::json(&types::TickSummary::from(&report));
    }
    println!(
        "plan {plan_id}: launched={} completed={} failed={} finished={}",
        report.launched.len(),
        report.completed.len(),
        report.failed.len(),
        report.finished
    );
    Ok(())
}

fn interrupt(parsed: args::PlanArguments, context: &RuntimeContext) -> Result<()> {
    let plan_id = required_plan_id(&parsed)?;
    let factory = lifecycle::ProductionPlanFactory::new(
        context.paths.clone(),
        context.settings.clone(),
        parsed.scope.clone(),
        context.provider_runtime()?,
    );
    parsed
        .runtime
        .validate_against_settings(&context.settings, Some(factory.eligible_account_count()))?;
    factory.interrupt(&plan_id, parsed.runtime.runtime, parsed.error)?;
    render(&status::plan_status(&context.paths, &plan_id)?, parsed.json)
}

fn recover(parsed: args::PlanArguments, context: &RuntimeContext) -> Result<()> {
    let plan_id = required_plan_id(&parsed)?;
    let factory = lifecycle::ProductionPlanFactory::new(
        context.paths.clone(),
        context.settings.clone(),
        parsed.scope,
        context.provider_runtime()?,
    );
    parsed
        .runtime
        .validate_against_settings(&context.settings, Some(factory.eligible_account_count()))?;
    let report = factory.recover(&plan_id, parsed.runtime.runtime)?;
    if parsed.json {
        return crate::output::json(&serde_json::json!({
            "waiting": report.waiting,
            "completed": report.completed,
            "failed": report.failed,
            "retried": report.retried,
        }));
    }
    println!("{report:?}");
    Ok(())
}

fn status(parsed: args::PlanArguments, context: &RuntimeContext) -> Result<()> {
    if let Some(plan_id) = parsed.plan_id {
        return render(&status::plan_status(&context.paths, &plan_id)?, parsed.json);
    }
    render(&status::plan_statuses(&context.paths)?, parsed.json)
}

fn required_plan_id(parsed: &args::PlanArguments) -> Result<String> {
    parsed
        .plan_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("scheduler command requires a plan id"))
}

fn spawn_scheduler_detached(parsed: &args::PlanArguments, plan_id: &str) -> Result<()> {
    let executable = std::env::current_exe()?;
    let mut command = std::process::Command::new(executable);
    scrub_provider_environment(&mut command);
    let args = [
        "run-all".to_owned(),
        "attach".to_owned(),
        plan_id.to_owned(),
        "--wait".to_owned(),
        "--workers".to_owned(),
        parsed.scope.csv(),
        "--max-live".to_owned(),
        parsed.runtime.runtime.max_live.to_string(),
        "--max-stall-cycles".to_owned(),
        parsed.runtime.runtime.max_stall_cycles.to_string(),
        "--max-attempts".to_owned(),
        parsed.runtime.runtime.max_attempts.to_string(),
        "--max-ticks".to_owned(),
        parsed.runtime.max_ticks.to_string(),
    ];
    command.args(args);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let child = crate::process::spawn_detached(&mut command)?;
    drop(child);
    Ok(())
}

fn render<T: serde::Serialize + std::fmt::Debug>(value: &T, json: bool) -> Result<()> {
    if json {
        crate::output::json(value)
    } else {
        println!("{value:?}");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
