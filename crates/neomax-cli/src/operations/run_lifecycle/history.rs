use anyhow::Result;
use neomax_core::runs::{ArchivedRun, HistoryStore, HistorySummary};
use serde::Serialize;

use super::RunLifecycleReport;
use super::RunView;
use super::logs;
use super::options;
use crate::context::RuntimeContext;
use crate::error;

#[derive(Debug, Serialize)]
pub(crate) struct HistoryReport {
    pub rows: Vec<HistorySummary>,
    pub detail: Option<HistoryDetail>,
    pub log: Option<logs::LogReport>,
}

#[derive(Debug, Serialize)]
pub(crate) struct HistoryDetail {
    pub run: RunView,
    pub archived_status: String,
    pub log_path: Option<String>,
}

pub(crate) fn history(context: &RuntimeContext, args: &[String]) -> Result<RunLifecycleReport> {
    let store = history_store(context);
    let values = error::usage(options::positional(args, &["--json", "--log"]))?;
    let engine = error::usage(options::engine(args))?;
    if let Some(id) = values
        .first()
        .filter(|value| value.parse::<usize>().is_err())
    {
        if !options::valid_run_id(id) {
            anyhow::bail!("run id contains unsafe path characters");
        }
        let archived = store
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("no archived run {id}"))?;
        let detail = detail(&archived);
        let log = if crate::parser::has(args, "--log") {
            archived
                .log_path
                .as_deref()
                .map(|path| logs::read_archived_log(context, id, path))
                .transpose()?
        } else {
            None
        };
        return Ok(RunLifecycleReport::History(HistoryReport {
            rows: Vec::new(),
            detail: Some(detail),
            log,
        }));
    }
    let limit = error::usage(options::limit(args, 40))?;
    let rows = store.list(limit, engine)?;
    Ok(RunLifecycleReport::History(HistoryReport {
        rows,
        detail: None,
        log: None,
    }))
}

fn history_store(context: &RuntimeContext) -> HistoryStore {
    HistoryStore::new(
        &context.paths.history_db,
        &context.paths.logs,
        &context.paths.history_logs,
        &context.paths.history_pending,
    )
}

fn detail(archived: &ArchivedRun) -> HistoryDetail {
    let probe = neomax_core::runs::SystemProcessProbe;
    HistoryDetail {
        run: RunView::from_record(&archived.run, &probe),
        archived_status: archived.status.as_str().to_owned(),
        log_path: archived
            .log_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
    }
}
