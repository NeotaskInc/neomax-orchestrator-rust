use std::collections::BTreeSet;
use std::time::Duration;

use anyhow::{Result, bail};
use neomax_core::Error as CoreError;
use neomax_core::runs::reconciliation::{
    HealResult, ReconcileRequest, ReconciliationService, RepairAction, RepairExecutor, RepairPlan,
    SelfHealPolicy, SelfHealStore,
};
use neomax_core::runs::{RunRecord, RunStatus, RunStore, SystemProcessProbe, effective_status};
use serde_json::json;

use super::super::args;
use super::shared::{history_store, owned_by_other, run_match};
use crate::context::RuntimeContext;
use crate::output;

pub(crate) fn reconcile(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(
        args,
        &["--project", "--limit", "--max-age-hours", "--max"],
        &["--json", "--heal", "--allow-repeat", "--any"],
    )?;
    let project = parsed.value("--project").map(str::to_owned);
    let limit = parsed
        .value("--limit")
        .map_or(Ok(usize::MAX), |value| args::positive(value, "--limit"))?;
    let max_heal = parsed
        .value("--max")
        .map_or(Ok(SelfHealPolicy::default().max_batch), |value| {
            args::positive(value, "--max")
        })?;
    let max_age_hours = parsed.value("--max-age-hours").map_or(Ok(6.0), |value| {
        let hours = value
            .parse::<f64>()
            .map_err(|_| anyhow::anyhow!("--max-age-hours must be a positive number"))?;
        if !hours.is_finite() || hours <= 0.0 {
            bail!("--max-age-hours must be a positive number");
        }
        Ok(hours)
    })?;
    let run_store = RunStore::new(&context.paths.runs);
    let runs = run_store.all()?;
    let probe = SystemProcessProbe;
    let unresolved = runs
        .iter()
        .filter(|run| {
            project
                .as_deref()
                .is_none_or(|expected| run.project.as_deref() == Some(expected))
        })
        .filter(|run| {
            let status = effective_status(run, &probe);
            matches!(
                status,
                RunStatus::Running | RunStatus::Orphaned | RunStatus::Unknown
            ) || (status.is_terminal() && !run.is_acknowledged())
                || run.worktree_state.as_deref() == Some("has_changes")
        })
        .map(run_match)
        .take(limit)
        .collect::<Vec<_>>();
    let history = history_store(context);
    let archived_pending = if parsed.has("--heal") {
        history.reconcile_pending(context.now)?
    } else {
        Vec::new()
    };
    let self_heal = if parsed.has("--heal") {
        let excluded = runs
            .iter()
            .filter_map(|run| {
                let outside_project = project
                    .as_deref()
                    .is_some_and(|expected| run.project.as_deref() != Some(expected));
                let owned_elsewhere = if parsed.has("--any") {
                    false
                } else {
                    owned_by_other(context, run).unwrap_or(true)
                };
                (outside_project || owned_elsewhere).then_some(run.id.clone())
            })
            .collect::<BTreeSet<_>>();
        let policy = SelfHealPolicy {
            max_batch: max_heal,
            max_age: Duration::from_secs((max_age_hours * 3600.0) as u64),
            ..SelfHealPolicy::default()
        };
        let request = ReconcileRequest {
            now: context.now,
            policy,
            allow_repeat: parsed.has("--allow-repeat"),
            excluded_run_ids: excluded,
        };
        let ledger = SelfHealStore::at(&context.paths.self_heal, &context.paths.self_heal_lock);
        let service = ReconciliationService::new(&run_store, &probe, &ledger);
        let executor = NativeRepairExecutor { context };
        Some(service.reconcile(&request, Some(&executor))?)
    } else {
        None
    };
    let report = json!({
        "unresolved": unresolved,
        "archived_pending": archived_pending,
        "heal": parsed.has("--heal"),
        "self_heal": self_heal,
    });
    if parsed.has("--json") {
        return output::json(&report);
    }
    if unresolved.is_empty() {
        println!("reconcile: no unresolved runs");
    } else {
        println!("reconcile: {} unresolved run(s)", unresolved.len());
        for run in unresolved {
            println!("  {} {} {}", run.id, run.status, run.prompt);
        }
    }
    if !archived_pending.is_empty() {
        println!(
            "reconcile: archived {} pending history record(s)",
            archived_pending.len()
        );
    }
    if let Some(self_heal) = self_heal {
        print_heal_summary(&self_heal.healed);
    }
    Ok(())
}

struct NativeRepairExecutor<'a> {
    context: &'a RuntimeContext,
}

impl RepairExecutor for NativeRepairExecutor<'_> {
    fn execute(&self, plan: &RepairPlan, _run: &RunRecord) -> neomax_core::Result<()> {
        let id = plan.run_id.clone();
        if plan.class == neomax_core::runs::reconciliation::ReconcileClass::Orphaned {
            run_lifecycle(
                self.context,
                crate::operations::run_lifecycle::RunLifecycleCommand::Kill,
                &id,
            )?;
        }
        let command = match plan.action {
            RepairAction::Resume => crate::operations::run_lifecycle::RunLifecycleCommand::Resume,
            RepairAction::Retry => crate::operations::run_lifecycle::RunLifecycleCommand::Retry,
            RepairAction::Kill => crate::operations::run_lifecycle::RunLifecycleCommand::Kill,
        };
        run_lifecycle(self.context, command, &id)
    }
}

fn run_lifecycle(
    context: &RuntimeContext,
    command: crate::operations::run_lifecycle::RunLifecycleCommand,
    id: &str,
) -> neomax_core::Result<()> {
    crate::operations::run_lifecycle::execute_native_report(command, context, &[id.to_owned()])
        .map(|_| ())
        .map_err(|error| CoreError::Message(error.to_string()))
}

fn print_heal_summary(healed: &[HealResult]) {
    if healed.is_empty() {
        return;
    }
    let completed = healed.iter().filter(|result| result.completed).count();
    println!(
        "reconcile --heal: dispatched {} repair(s), {} completed",
        healed.len(),
        completed
    );
    for result in healed {
        let status = if result.completed { "ok" } else { "failed" };
        println!(
            "  {} {} attempt={} {}",
            result.run_id, result.action, result.attempt, status
        );
    }
}
