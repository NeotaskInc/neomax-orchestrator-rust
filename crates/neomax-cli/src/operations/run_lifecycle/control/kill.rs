use anyhow::Result;
use neomax_core::runs::{HistoryStore, RunStatus, RunStore, effective_status};
use serde::Serialize;

use super::events;
use crate::context::RuntimeContext;
use crate::error;
use crate::operations::run_lifecycle::RunLifecycleReport;
use crate::operations::run_lifecycle::options;
use crate::operations::run_lifecycle::process::{ProcessControl, ProcessTarget, validate_pid};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KillTarget {
    pid: u32,
    kind: ProcessTarget,
}

#[derive(Debug, Serialize)]
pub(crate) struct KillReport {
    pub id: String,
    pub status: String,
    pub marked: bool,
    pub terminated: bool,
    pub target: Option<String>,
    pub acknowledged: bool,
    pub archived: bool,
    pub worktree_preserved: bool,
    pub message: String,
}

pub(crate) fn run(
    context: &RuntimeContext,
    args: &[String],
    process: &dyn ProcessControl,
) -> Result<RunLifecycleReport> {
    let all = crate::parser::has(args, "--all");
    let values = error::usage(options::positional(args, &["--all", "--json", "--any"]))?;
    if all && !values.is_empty() {
        anyhow::bail!("kill --all cannot be combined with a run id");
    }
    let ids = if all {
        RunStore::new(&context.paths.runs)
            .all()?
            .into_iter()
            .filter(|run| {
                matches!(
                    effective_status(run, process),
                    RunStatus::Running | RunStatus::Orphaned
                )
            })
            .map(|run| run.id)
            .collect::<Vec<_>>()
    } else {
        vec![error::usage(options::run_id(args))?]
    };
    let mut reports = Vec::with_capacity(ids.len());
    for id in ids {
        reports.push(kill_one(context, &id, process)?);
    }
    if reports.len() == 1 {
        let report = reports
            .pop()
            .ok_or_else(|| anyhow::anyhow!("kill produced no report"))?;
        return Ok(RunLifecycleReport::Kill(report));
    }
    let processed = !reports.is_empty();
    Ok(RunLifecycleReport::Kill(KillReport {
        id: "all".into(),
        status: "aborted".into(),
        marked: processed && reports.iter().all(|report| report.marked),
        terminated: reports.iter().all(|report| report.terminated),
        target: None,
        acknowledged: false,
        archived: processed && reports.iter().all(|report| report.archived),
        worktree_preserved: processed && reports.iter().all(|report| report.worktree_preserved),
        message: format!("processed {} running run(s)", reports.len()),
    }))
}

fn kill_one(
    context: &RuntimeContext,
    id: &str,
    process: &dyn ProcessControl,
) -> Result<KillReport> {
    let store = RunStore::new(&context.paths.runs);
    let mut target = None;
    let mut changed = false;
    let updated = store.update(id, |persisted| {
        target = live_target(persisted, process);
        if persisted.status.is_terminal() && target.is_none() {
            return Ok(());
        }
        changed = true;
        if !persisted.status.is_terminal() {
            persisted.killed = true;
            persisted.status = RunStatus::Aborted;
            persisted.ended = Some(context.now);
            persisted.acknowledged = Some(false);
            persisted.interrupt_signal = Some(15);
        }
        Ok(())
    })?;

    if changed {
        events::append(context, &updated, "killed");
    }

    let (terminated, message) = match target {
        Some(KillTarget { pid, kind }) => match process.terminate(pid, kind) {
            Ok(()) => {
                let cleanup = store.update(id, |persisted| {
                    match kind {
                        ProcessTarget::Supervisor if persisted.supervisor_pid == Some(pid) => {
                            persisted.supervisor_pid = None;
                        }
                        ProcessTarget::Worker if persisted.worker_pid == Some(pid) => {
                            persisted.worker_pid = None;
                        }
                        _ => {}
                    }
                    Ok(())
                });
                match cleanup {
                    Ok(_) => (true, format!("termination requested for process {pid}")),
                    Err(error) => (
                        false,
                        format!(
                            "termination requested for process {pid}, but state cleanup failed: {error}"
                        ),
                    ),
                }
            }
            Err(error) => (false, error.to_string()),
        },
        None => (true, "no live provider process was found".into()),
    };

    let current = store.load(id)?;
    let archived = if changed
        && current.status == RunStatus::Aborted
        && live_target(&current, process).is_none()
    {
        let history = HistoryStore::new(
            &context.paths.history_db,
            &context.paths.logs,
            &context.paths.history_logs,
            &context.paths.history_pending,
        );
        history
            .archive_or_spill(&current, None, context.now)
            .is_ok()
    } else {
        false
    };
    Ok(KillReport {
        id: id.into(),
        status: effective_status(&current, process).as_str().into(),
        marked: updated.killed,
        terminated,
        target: target.map(|target| match target.kind {
            ProcessTarget::Supervisor => "supervisor".into(),
            ProcessTarget::Worker => "worker".into(),
        }),
        acknowledged: current.is_acknowledged(),
        archived,
        worktree_preserved: current.worktree.is_some(),
        message,
    })
}

fn live_target(
    run: &neomax_core::runs::RunRecord,
    process: &dyn ProcessControl,
) -> Option<KillTarget> {
    run.worker_pid
        .filter(|pid| safe_pid(*pid))
        .filter(|pid| process.worker_alive(*pid, run.engine))
        .map(|pid| KillTarget {
            pid,
            kind: ProcessTarget::Worker,
        })
        .or_else(|| {
            run.supervisor_pid
                .filter(|pid| safe_pid(*pid))
                .filter(|pid| process.pid_alive(*pid))
                .map(|pid| KillTarget {
                    pid,
                    kind: ProcessTarget::Supervisor,
                })
        })
}

fn safe_pid(pid: u32) -> bool {
    validate_pid(pid).is_ok() && pid != std::process::id()
}
