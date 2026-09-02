use std::collections::BTreeMap;
use std::env;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use neomax_core::orchestration::registry::{OrchestratorStore, owned_by_other_live_orchestrator};
use neomax_core::runs::{EventStore, HistoryStore, RunEvent, RunRecord, SystemProcessProbe};
use serde::Serialize;

use crate::context::RuntimeContext;

#[derive(Debug, Clone, Serialize)]
pub(super) struct RunMatch {
    pub(super) id: String,
    pub(super) engine: String,
    pub(super) model: String,
    pub(super) account: String,
    pub(super) status: String,
    pub(super) session: Option<String>,
    pub(super) prompt: String,
    pub(super) branch: Option<String>,
    pub(super) repo: Option<String>,
    pub(super) files: Vec<String>,
}

pub(super) fn run_match(run: &RunRecord) -> RunMatch {
    RunMatch {
        id: run.id.clone(),
        engine: run.engine.to_string(),
        model: run.model.clone(),
        account: run.account(),
        status: run.status.as_str().into(),
        session: run.session.clone(),
        prompt: run.prompt.clone(),
        branch: run.branch.clone(),
        repo: run.repo.as_ref().map(|path| path.display().to_string()),
        files: run.files_touched.clone(),
    }
}

pub(super) fn searchable_fields(run: &RunRecord) -> Vec<String> {
    let mut values = vec![
        run.prompt.clone(),
        run.model.clone(),
        run.profile.display().to_string(),
        run.workdir.display().to_string(),
    ];
    values.extend(run.files_touched.iter().cloned());
    values.extend(run.branch.iter().cloned());
    values.extend(run.repo.iter().map(|path| path.display().to_string()));
    values.extend(run.project.iter().cloned());
    values
}

pub(super) fn owned_by_other(context: &RuntimeContext, run: &RunRecord) -> Result<bool> {
    let current = env::var("NEOMAX_ORCH_SESSION").ok();
    let records = OrchestratorStore::new(&context.paths.orchestrators)
        .all(&SystemProcessProbe, context.now)?;
    Ok(owned_by_other_live_orchestrator(
        run,
        current.as_deref(),
        &records,
    ))
}

pub(super) fn append_event(
    context: &RuntimeContext,
    run: &RunRecord,
    event: &str,
    extra: BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    let now = Utc
        .timestamp_opt(context.now, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("invalid event timestamp"))?;
    EventStore::with_legacy_directory(&context.paths.run_events, &context.paths.events).append(
        &RunEvent {
            ts: context.now,
            run: run.id.clone(),
            event: event.into(),
            engine: run.engine,
            account: Some(run.account()),
            status: Some(run.status),
            attempt: Some(run.attempt),
            extra,
        },
        now,
    )?;
    Ok(())
}

pub(super) fn history_store(context: &RuntimeContext) -> HistoryStore {
    HistoryStore::new(
        &context.paths.history_db,
        &context.paths.logs,
        &context.paths.history_logs,
        &context.paths.history_pending,
    )
}

pub(super) fn format_timestamp(timestamp: i64) -> String {
    Utc.timestamp_opt(timestamp, 0).single().map_or_else(
        || timestamp.to_string(),
        |value| value.format("%m-%d %H:%M:%S").to_string(),
    )
}
