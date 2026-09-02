use neomax_core::scheduler::PartState;
use neomax_core::scheduler::persistence::{PlanRecord, PlanStatus, PlanStore};
use neomax_core::{Result, StatePaths};
use serde::Serialize;

use super::types::{PartStatusView, PlanRunReport};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PlanStatusView {
    pub plan_id: String,
    pub status: PlanStatus,
    pub repository: Option<String>,
    pub base: Option<String>,
    pub integration_branch: Option<String>,
    pub worktree: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub updated_at: i64,
    pub ended_at: Option<i64>,
    pub error: Option<String>,
    pub recovery_count: u32,
    pub killed: bool,
    pub interrupted: bool,
    pub parts: Vec<PartStatusView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PlanStatusReport {
    pub plans: Vec<PlanStatusView>,
}

pub(crate) fn plan_status(paths: &StatePaths, plan_id: &str) -> Result<PlanStatusView> {
    let store = PlanStore::new(&paths.plans);
    let record = store.load(plan_id)?;
    Ok(view_record(&record))
}

pub(crate) fn plan_statuses(paths: &StatePaths) -> Result<PlanStatusReport> {
    let store = PlanStore::new(&paths.plans);
    Ok(PlanStatusReport {
        plans: store.all()?.iter().map(view_record).collect(),
    })
}

pub(crate) fn run_report(
    plan_id: &str,
    record: &PlanRecord,
    ticks: usize,
    last_tick: Option<super::types::TickSummary>,
) -> PlanRunReport {
    PlanRunReport {
        plan_id: plan_id.to_owned(),
        status: status_name(record.status).to_owned(),
        finished: record.state.finished(),
        ticks,
        last_tick,
        state: record.state.clone(),
    }
}

fn view_record(record: &PlanRecord) -> PlanStatusView {
    let parts = record
        .plan
        .parts
        .iter()
        .map(|part| {
            let execution = record.state.execution(&part.id);
            PartStatusView {
                id: part.id.clone(),
                engine: part.engine,
                status: record.state.state(&part.id).unwrap_or(PartState::Pending),
                run_id: execution.and_then(|value| value.run_id.clone()),
                branch: execution.and_then(|value| value.branch.clone()),
                profile: execution.and_then(|value| value.profile.clone()),
            }
        })
        .collect();
    PlanStatusView {
        plan_id: record.plan_id.clone(),
        status: record.status,
        repository: record
            .repository
            .as_ref()
            .map(|value| value.display().to_string()),
        base: record.base.clone(),
        integration_branch: record.integration_branch.clone(),
        worktree: record
            .worktree
            .as_ref()
            .map(|value| value.display().to_string()),
        created_at: record.created_at,
        started_at: record.started_at,
        updated_at: record.updated_at,
        ended_at: record.ended_at,
        error: record.error.clone(),
        recovery_count: record.recovery_count,
        killed: record.killed,
        interrupted: record.interrupted,
        parts,
    }
}

fn status_name(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Pending => "pending",
        PlanStatus::Running => "running",
        PlanStatus::Done => "done",
        PlanStatus::Failed => "failed",
        PlanStatus::Interrupted => "interrupted",
        PlanStatus::Killed => "killed",
        PlanStatus::Unknown => "unknown",
    }
}
