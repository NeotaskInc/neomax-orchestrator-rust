use crate::parser;

use super::LaunchPlan;

pub(crate) fn print_text(plan: &LaunchPlan) {
    print!("{}", text(plan));
}

pub(crate) fn text(plan: &LaunchPlan) -> String {
    let mut lines = vec![
        format!("launch plan: {}", plan.invocation),
        format!("mode = {:?}", plan.mode),
        format!(
            "orchestrator = {}",
            plan.orchestrator.as_deref().unwrap_or("dynamic")
        ),
        format!("workers = {}", plan.worker_engines.join(",")),
        format!("routing = {}", plan.routing),
        format!("plan_mode = {}", plan.plan_mode),
    ];
    if let Some(account) = &plan.account {
        lines.push(format!("account = {account}"));
    }
    if let Some(run_id) = &plan.run_id {
        lines.push(format!("run_id = {run_id}"));
    }
    if let Some(tag) = &plan.tag {
        lines.push(format!("tag = {tag}"));
    }
    if let Some(operation) = &plan.operation {
        lines.push(format!("operation = {operation}"));
        if !plan.operation_args.is_empty() {
            lines.push(format!(
                "operation_args = {}",
                plan.operation_args.join(" ")
            ));
        }
    }
    lines.push(format!(
        "initial_task = {}",
        plan.initial_task.as_deref().unwrap_or("(none)")
    ));
    if plan.plan_mode {
        lines.push(
            "plan_guarantees = current checkout; no managed worktree; provider read-only boundary; no provider execution in this dry-run".into(),
        );
    }
    lines.push("models:".into());
    for model in plan.models.values() {
        lines.push(format!(
            "  {} = {} ({})",
            model.engine, model.model, model.source
        ));
    }
    lines.push("adapters:".into());
    for adapter in &plan.adapters {
        lines.push(format!(
            "  {} -> {} [{}; {}]",
            adapter.provider, adapter.executable, adapter.role, adapter.execution
        ));
        lines.push(format!("    environment = {}", adapter.environment.source));
    }
    lines.push(format!("provider_execution = {}", plan.provider_execution));
    lines.join("\n") + "\n"
}

pub(crate) fn plan_is_json(args: &[String]) -> bool {
    parser::has(args, "--json")
}
