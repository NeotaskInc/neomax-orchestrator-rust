use std::env;

use anyhow::Result;
use neomax_core::queue::AgentQueue;
use serde_json::json;

use crate::context::RuntimeContext;
use crate::error;
use crate::output;
use crate::parser;

pub fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err(error::usage_error(anyhow::anyhow!(
            "usage: neomax queue <status|reserve|poll|release|set-budget> ..."
        )));
    };
    let rest = &args[1..];
    let queue = AgentQueue::from_settings(&context.paths.agent_queue, &context.settings);
    match subcommand {
        "status" => status(context, &queue, rest),
        "reserve" => reserve(context, &queue, rest),
        "poll" => poll(context, &queue, rest),
        "release" => release(context, &queue, rest),
        "set-budget" => set_budget(context, &queue, rest),
        other => Err(error::usage_error(anyhow::anyhow!(
            "queue: unknown subcommand {other}"
        ))),
    }
}

fn status(context: &RuntimeContext, queue: &AgentQueue, args: &[String]) -> Result<()> {
    let state = queue.snapshot(context.now as f64, &context.liveness)?;
    let metrics = state.metrics();
    if parser::has(args, "--json") {
        return output::json(&json!({
            "agent_budget": metrics.agent_budget,
            "task_budget": metrics.task_budget,
            "used": metrics.used,
            "free": metrics.free,
            "active_tasks": metrics.active_tasks,
            "queued_tasks": metrics.queued_tasks,
            "queue": state.queue,
        }));
    }
    println!(
        "agent budget {} | used {} | free {} | tasks {} (cap {})",
        metrics.agent_budget,
        metrics.used,
        metrics.free,
        metrics.queued_tasks,
        if metrics.task_budget == 0 {
            "unlimited".to_owned()
        } else {
            metrics.task_budget.to_string()
        }
    );
    for reservation in state.queue {
        let waiting = reservation.want.saturating_sub(reservation.granted);
        let batch = reservation
            .batch
            .map(|value| format!(" [{value}]"))
            .unwrap_or_default();
        if waiting == 0 {
            println!(
                "  {:<26} want {:>3}  granted {:>3}  running{}",
                reservation.task, reservation.want, reservation.granted, batch
            );
        } else {
            println!(
                "  {:<26} want {:>3}  granted {:>3}  WAITING on {}{}",
                reservation.task, reservation.want, reservation.granted, waiting, batch
            );
        }
    }
    Ok(())
}

fn reserve(context: &RuntimeContext, queue: &AgentQueue, args: &[String]) -> Result<()> {
    let task = error::usage(parser::value(args, "--task"))?
        .ok_or_else(|| error::usage_error(anyhow::anyhow!("queue reserve: --task is required")))?;
    let raw_agents = error::usage(parser::value(args, "--agents"))?.ok_or_else(|| {
        error::usage_error(anyhow::anyhow!("queue reserve: --agents is required"))
    })?;
    let agents = error::usage(parser::parse_positive_u32(
        &raw_agents,
        "queue reserve: --agents",
    ))?;
    let reservation = queue.reserve(
        &task,
        agents,
        &session_id(),
        error::usage(parser::value(args, "--batch"))?,
        context.now as f64,
        &context.liveness,
    )?;
    let waiting = reservation.want.saturating_sub(reservation.granted);
    if parser::has(args, "--json") {
        return output::json(&json!({
            "id": reservation.id,
            "task": reservation.task,
            "want": reservation.want,
            "granted": reservation.granted,
            "waiting": waiting,
        }));
    }
    eprintln!(
        "queue: {} reserved {}, GRANTED {}{}",
        reservation.task,
        reservation.want,
        reservation.granted,
        if waiting == 0 {
            String::new()
        } else {
            format!(" (waiting on {waiting})")
        }
    );
    println!("{}", reservation.granted);
    Ok(())
}

fn poll(context: &RuntimeContext, queue: &AgentQueue, args: &[String]) -> Result<()> {
    let id = error::usage(parser::value(args, "--id"))?;
    let task = error::usage(parser::value(args, "--task"))?;
    if id.is_none() && task.is_none() {
        return Err(error::usage_error(anyhow::anyhow!(
            "queue poll: --id or --task is required"
        )));
    }
    let reservation = queue.poll(
        id.as_deref(),
        task.as_deref(),
        context.now as f64,
        &context.liveness,
    )?;
    let Some(reservation) = reservation else {
        if parser::has(args, "--json") {
            println!("null");
        } else {
            println!("(no such reservation)");
        }
        return Ok(());
    };
    let waiting = reservation.want.saturating_sub(reservation.granted);
    if parser::has(args, "--json") {
        return output::json(&json!({
            "id": reservation.id,
            "task": reservation.task,
            "want": reservation.want,
            "granted": reservation.granted,
            "waiting": waiting,
        }));
    }
    println!("{}", reservation.granted);
    Ok(())
}

fn release(context: &RuntimeContext, queue: &AgentQueue, args: &[String]) -> Result<()> {
    let id = error::usage(parser::value(args, "--id"))?;
    let task = error::usage(parser::value(args, "--task"))?;
    if id.is_none() && task.is_none() {
        return Err(error::usage_error(anyhow::anyhow!(
            "queue release: --id or --task is required"
        )));
    }
    let released = queue.release(
        id.as_deref(),
        task.as_deref(),
        context.now as f64,
        &context.liveness,
    )?;
    println!("released {released} reservation(s)");
    Ok(())
}

fn set_budget(context: &RuntimeContext, queue: &AgentQueue, args: &[String]) -> Result<()> {
    let agents = error::usage(parser::value(args, "--agents"))?
        .map(|value| value.parse::<u32>())
        .transpose()
        .map_err(|_| {
            error::usage_error(anyhow::anyhow!(
                "queue set-budget: --agents must be an integer"
            ))
        })?;
    let tasks = error::usage(parser::value(args, "--tasks"))?
        .map(|value| value.parse::<u32>())
        .transpose()
        .map_err(|_| {
            error::usage_error(anyhow::anyhow!(
                "queue set-budget: --tasks must be an integer"
            ))
        })?;
    if agents.is_none() && tasks.is_none() {
        return Err(error::usage_error(anyhow::anyhow!(
            "queue set-budget: --agents or --tasks is required"
        )));
    }
    let state = queue.set_budgets(agents, tasks, context.now as f64, &context.liveness)?;
    println!(
        "agent_budget={} task_budget={}",
        state.agent_budget,
        if state.task_budget == 0 {
            "unlimited".to_owned()
        } else {
            state.task_budget.to_string()
        }
    );
    Ok(())
}

fn session_id() -> String {
    env::var("NEOMAX_ORCH_SESSION")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("pid-{}", std::process::id()))
}
