use neomax_core::runs::{RunRecord, RunStore, in_inbox};
use serde::Serialize;

use super::RunLifecycleReport;
use super::RunView;
use super::options;
use super::process::ProcessControl;
use crate::context::RuntimeContext;
use crate::error;

#[derive(Debug, Serialize)]
pub(crate) struct RunListReport {
    pub runs: Vec<RunView>,
    pub inbox: usize,
    pub orphaned: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct RunStatusReport {
    pub now: i64,
    pub runs: Vec<RunView>,
    pub running: usize,
    pub orphaned: usize,
    pub inbox: usize,
}

pub(crate) fn list(
    context: &RuntimeContext,
    args: &[String],
    process: &dyn ProcessControl,
) -> anyhow::Result<RunLifecycleReport> {
    let values = error::usage(options::positional(args, &["--json", "--hook"]))?;
    if !values.is_empty() {
        anyhow::bail!("ls does not accept positional arguments");
    }
    let engine = error::usage(options::engine(args))?;
    let status = error::usage(options::status(args))?;
    let limit = error::usage(options::limit(args, 10_000))?;
    let matches_run = |run: &RunRecord| {
        engine.is_none_or(|value| run.engine == value)
            && status
                .as_deref()
                .is_none_or(|value| effective_status_name(run, process) == value)
    };
    let hook = crate::parser::has(args, "--hook");
    if hook && std::env::var_os("NEOMAX_WORKER").is_some() {
        return Ok(RunLifecycleReport::List(RunListReport {
            runs: Vec::new(),
            inbox: 0,
            orphaned: 0,
        }));
    }
    let records = RunStore::new(&context.paths.runs).all()?;
    let mut views = records
        .iter()
        .filter(|run| matches_run(run))
        .filter(|run| !hook || unfinished(run, process))
        .map(|run| RunView::from_record(run, process))
        .collect::<Vec<_>>();
    views.sort_by(|left, right| {
        right
            .started
            .cmp(&left.started)
            .then_with(|| left.id.cmp(&right.id))
    });
    views.truncate(limit);
    let inbox = records
        .iter()
        .filter(|run| matches_run(run))
        .filter(|run| in_inbox(run, process))
        .count();
    let orphaned = records
        .iter()
        .filter(|run| matches_run(run))
        .filter(|run| effective_status_name(run, process) == "orphaned")
        .count();
    Ok(RunLifecycleReport::List(RunListReport {
        runs: views,
        inbox,
        orphaned,
    }))
}

pub(crate) fn status(
    context: &RuntimeContext,
    args: &[String],
    process: &dyn ProcessControl,
) -> anyhow::Result<RunLifecycleReport> {
    let values = error::usage(options::positional(args, &["--json"]))?;
    if !values.is_empty() {
        anyhow::bail!("status does not accept positional arguments");
    }
    let engine = error::usage(options::engine(args))?;
    let status = error::usage(options::status(args))?;
    let limit = error::usage(options::limit(args, 10_000))?;
    let matches_run = |run: &RunRecord| {
        engine.is_none_or(|value| run.engine == value)
            && status
                .as_deref()
                .is_none_or(|value| effective_status_name(run, process) == value)
    };
    let records = RunStore::new(&context.paths.runs).all()?;
    let mut runs = records
        .iter()
        .filter(|run| matches_run(run))
        .map(|run| RunView::from_record(run, process))
        .collect::<Vec<_>>();
    runs.sort_by(|left, right| {
        right
            .started
            .cmp(&left.started)
            .then_with(|| left.id.cmp(&right.id))
    });
    let running = runs.iter().filter(|run| run.status == "running").count();
    let orphaned = runs.iter().filter(|run| run.status == "orphaned").count();
    runs.truncate(limit);
    let inbox = records
        .iter()
        .filter(|run| matches_run(run))
        .filter(|run| in_inbox(run, process))
        .count();
    Ok(RunLifecycleReport::Status(RunStatusReport {
        now: context.now,
        runs,
        running,
        orphaned,
        inbox,
    }))
}

fn unfinished(run: &RunRecord, process: &dyn ProcessControl) -> bool {
    let status = effective_status_name(run, process);
    matches!(status, "running" | "orphaned")
        || run.worktree_state.as_deref() == Some("has_changes")
        || in_inbox(run, process)
}

fn effective_status_name(run: &RunRecord, process: &dyn ProcessControl) -> &'static str {
    neomax_core::runs::effective_status(run, process).as_str()
}
