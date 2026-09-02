use anyhow::Result;
use neomax_core::orchestration::registry::OrchestratorStore;
use neomax_core::queue::AgentQueue;
use neomax_core::runs::{RunStore, SystemProcessProbe};

use super::types::{
    AccountView, AmbientView, OrchestratorView, QueueView, RunView, SessionView, StatusReport,
    StatusSummary, SubagentView,
};
use crate::context::RuntimeContext;

pub(crate) fn build_report(context: &RuntimeContext) -> Result<StatusReport> {
    let probe = SystemProcessProbe;
    let runs_store = RunStore::new(&context.paths.runs);
    let records = runs_store.all()?;
    let run_views = records
        .iter()
        .map(|run| super::runs::view_run(run, &probe))
        .collect::<Vec<_>>();
    let sessions = records
        .iter()
        .filter_map(|run| super::runs::view_session(run, &probe))
        .collect::<Vec<_>>();
    let subagents = records
        .iter()
        .flat_map(|run| super::runs::view_subagents(run, &probe))
        .collect::<Vec<_>>();
    let orchestrator_records =
        OrchestratorStore::new(&context.paths.orchestrators).all(&probe, context.now)?;
    let queue = queue_view(context)?;
    let runtime = context.provider_runtime()?;
    let session_snapshot =
        super::sessions::discover(context, &runtime, &records, &orchestrator_records)?;
    let ambient = super::ambient::views(&session_snapshot);
    let accounts =
        super::accounts::views(context, &runtime, &runs_store, &probe, &session_snapshot)?;
    let orchestrators = orchestrator_records
        .into_iter()
        .map(super::ambient::orchestrator)
        .collect::<Vec<_>>();
    let engines = super::accounts::provider_views(&runtime, accounts.clone());
    let connected_engines = engines
        .values()
        .filter(|provider| provider.connected)
        .map(|provider| provider.engine.as_str().to_string())
        .collect::<Vec<_>>();
    let summary = summary(
        &accounts,
        &run_views,
        &sessions,
        &subagents,
        &ambient,
        &orchestrators,
        &queue,
    );
    Ok(StatusReport {
        now: context.now,
        engines,
        accounts,
        runs: run_views.clone(),
        run_ledger: run_views,
        sessions,
        ambient,
        subagents,
        orchestrators,
        queue,
        connected_engines,
        summary,
    })
}

fn queue_view(context: &RuntimeContext) -> Result<QueueView> {
    let queue = AgentQueue::from_settings(&context.paths.agent_queue, &context.settings)
        .snapshot(context.now as f64, &context.liveness)?;
    let metrics = queue.metrics();
    Ok(QueueView {
        agent_budget: metrics.agent_budget,
        task_budget: metrics.task_budget,
        used: metrics.used,
        free: metrics.free,
        active_tasks: metrics.active_tasks,
        queued_tasks: metrics.queued_tasks,
    })
}

fn summary(
    accounts: &[AccountView],
    runs: &[RunView],
    sessions: &[SessionView],
    subagents: &[SubagentView],
    ambient: &[AmbientView],
    orchestrators: &[OrchestratorView],
    queue: &QueueView,
) -> StatusSummary {
    let running = runs
        .iter()
        .filter(|run| run.status == "running" || run.status == "orphaned")
        .count();
    let workers = accounts.iter().map(|account| account.live_workers).sum();
    let agents_total = accounts.iter().map(|account| account.agents).sum();
    let native_sessions = ambient
        .iter()
        .filter(|session| {
            session.active && matches!(session.kind, neomax_core::sessions::SessionKind::Main)
        })
        .count();
    let native_subagents = ambient
        .iter()
        .filter(|session| {
            session.active && !matches!(session.kind, neomax_core::sessions::SessionKind::Main)
        })
        .count();
    StatusSummary {
        accounts_up: accounts
            .iter()
            .filter(|account| account.authenticated)
            .count(),
        accounts_total: accounts.len(),
        cooling: accounts
            .iter()
            .filter(|account| account.quota.cooldown_until.is_some())
            .count(),
        paused: accounts.iter().filter(|account| account.paused).count(),
        running,
        live_sessions: sessions.len() + native_sessions,
        subagents: subagents.len() + native_subagents,
        native_sessions,
        native_subagents,
        orchestrators: orchestrators.iter().filter(|record| record.live).count(),
        workers,
        agents_total,
        queued_tasks: queue.queued_tasks,
        inbox: runs.iter().filter(|run| !run.acknowledged).count(),
    }
}
