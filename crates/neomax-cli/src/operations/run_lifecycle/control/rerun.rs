use std::collections::BTreeSet;

use anyhow::{Result, bail};
use neomax_core::providers::catalog::supports_native_resume;
use neomax_core::runs::{RunStatus, RunStore, effective_status};

use super::events;
use super::ports::{RetryAccountSelector, RetrySelector, RunExecutor};
use crate::context::RuntimeContext;
use crate::operations::run_lifecycle::options;
use crate::operations::run_lifecycle::process::ProcessControl;
use crate::operations::run_lifecycle::{RunLifecycleReport, RunView};

pub(crate) fn run(
    context: &RuntimeContext,
    args: &[String],
    resume: bool,
    process: &dyn ProcessControl,
    executor: Option<&dyn RunExecutor>,
    retry_selector: Option<&dyn RetryAccountSelector>,
) -> Result<RunLifecycleReport> {
    let values = options::positional(args, &["--json", "--any"])?;
    let id = values
        .first()
        .filter(|value| options::valid_run_id(value))
        .ok_or_else(|| anyhow::anyhow!("a safe run id is required"))?
        .clone();
    let prompt_start = if resume { 1 } else { 2 };
    let continuation_prompt = values
        .iter()
        .skip(prompt_start)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    let executor = executor.ok_or_else(|| anyhow::anyhow!("run executor is not configured"))?;
    let selector = if resume {
        None
    } else {
        Some(match options::retry_selector(args)?.as_deref() {
            None | Some("auto") => RetrySelector::Auto,
            Some(account) => RetrySelector::Account(account.to_owned()),
        })
    };
    let selector = selector.as_ref();
    let selector_service = retry_selector.ok_or_else(|| {
        anyhow::anyhow!("retry requires an account selector backed by the provider inventory")
    });
    if !resume {
        selector_service?;
    }
    let store = RunStore::new(&context.paths.runs);
    let mut run = store.load(&id)?;
    let native_resume = resume && supports_native_resume(run.engine);
    if native_resume && run.session.is_none() {
        bail!("run {id} has no session id; use neomax retry {id} for a fresh attempt");
    }
    let current = effective_status(&run, process);
    if matches!(current, RunStatus::Running | RunStatus::Orphaned)
        || run
            .worker_pid
            .is_some_and(|pid| process.worker_alive(pid, run.engine))
    {
        bail!("run {id} still has a live worker or supervisor; kill it first");
    }
    if !run.workdir.as_os_str().is_empty() && !run.workdir.is_dir() {
        bail!("run {id} workdir does not exist: {}", run.workdir.display());
    }
    let mut excluded = BTreeSet::from([run.profile.clone()]);
    excluded.extend(run.tried.iter().cloned());
    let target = if let (Some(selector), Some(service)) = (selector, retry_selector) {
        Some(service.select(&run, selector, &excluded)?)
    } else {
        None
    };
    let updated = store.update(&id, |persisted| {
        let state = effective_status(persisted, process);
        if matches!(state, RunStatus::Running | RunStatus::Orphaned)
            || persisted
                .worker_pid
                .is_some_and(|pid| process.worker_alive(pid, persisted.engine))
        {
            return Err(neomax_core::Error::Conflict(format!(
                "run {id} became active while it was being prepared"
            )));
        }
        if persisted.tried.last() != Some(&persisted.profile) {
            persisted.tried.push(persisted.profile.clone());
        }
        if let Some(target) = target.as_ref() {
            persisted.profile = target.clone();
        }
        persisted.attempt = persisted.attempt.saturating_add(1);
        persisted.status = RunStatus::Running;
        persisted.ended = None;
        persisted.killed = false;
        persisted.acknowledged = Some(false);
        persisted.interrupt_signal = None;
        persisted.supervisor_pid = Some(std::process::id());
        persisted.worker_pid = None;
        if resume && !native_resume {
            persisted.remember_session();
            persisted.session = None;
        }
        persisted.resumed = native_resume;
        persisted.prompt_to_send = if resume {
            Some(if continuation_prompt.is_empty() {
                "Continue the task you were working on. Inspect the current worktree and finish the objective.".into()
            } else {
                continuation_prompt.clone()
            })
        } else if !continuation_prompt.is_empty() {
            Some(continuation_prompt.clone())
        } else {
            Some(format!(
                "{}\n\nContinue this task after the previous attempt. Inspect the current worktree before changing it.",
                persisted.prompt
            ))
        };
        persisted.resume_session = if native_resume {
            persisted.session.clone()
        } else {
            None
        };
        Ok(())
    })?;
    events::append(context, &updated, if resume { "resume" } else { "retry" });
    run = updated;
    let status = match executor.execute(&mut run) {
        Ok(status) => status,
        Err(error) => {
            let message = error.to_string();
            let _ = store.update(&id, |persisted| {
                persisted.status = RunStatus::Error;
                persisted.ended = Some(context.now);
                persisted.worker_pid = None;
                persisted.supervisor_pid = None;
                persisted.error_detail = Some(message.clone());
                Ok(())
            });
            return Err(error.into());
        }
    };
    run.status = status;
    run.ended = Some(context.now);
    run.worker_pid = None;
    run.supervisor_pid = None;
    run.resume_session = None;
    let saved = store.save_preserving_control_markers(&run)?;
    events::append(context, &saved, if resume { "resumed" } else { "retried" });
    Ok(RunLifecycleReport::Rerun(RunView::from_record(
        &saved, process,
    )))
}
