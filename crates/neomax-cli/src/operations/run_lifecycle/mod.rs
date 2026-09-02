mod control;
mod diff;
mod history;
mod listing;
mod logs;
mod options;
mod process;
mod render;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use neomax_core::accounts::{AccountControlStore, AccountInventory, SelectionPolicy};
use neomax_core::providers::runtime::ProviderRuntime;
use neomax_core::runs::{RunLiveWorkSource, RunStore, SystemProcessProbe};
use neomax_core::runs::{RunRecord, RunStatus};
use neomax_core::usage::UsageCacheStore;
use neomax_core::{EffectiveSettings, StatePaths, WorkerScope};
use serde::Serialize;

use crate::context::RuntimeContext;

pub(crate) use control::{InventoryRetrySelector, RetryAccountSelector, RunExecutor};
pub(crate) use process::{ProcessControl, SystemProcessControl};

struct NativeRunExecutor {
    paths: StatePaths,
    settings: EffectiveSettings,
    provider_runtime: ProviderRuntime,
}

impl RunExecutor for NativeRunExecutor {
    fn execute(&self, run: &mut RunRecord) -> neomax_core::Result<RunStatus> {
        crate::launch::execute_record_with_runtime(
            &self.provider_runtime,
            &self.paths,
            &self.settings,
            run,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunLifecycleCommand {
    List,
    Log,
    Resume,
    Retry,
    Kill,
    History,
    Status,
    Diff,
    SubagentDiff,
}

impl RunLifecycleCommand {
    pub(crate) const fn from_core(
        command: neomax_core::orchestration::commands::Command,
    ) -> Option<Self> {
        use neomax_core::orchestration::commands::Command;
        Some(match command {
            Command::List => Self::List,
            Command::Log => Self::Log,
            Command::Resume => Self::Resume,
            Command::Retry => Self::Retry,
            Command::Kill => Self::Kill,
            Command::History => Self::History,
            Command::Status => Self::Status,
            Command::Diff => Self::Diff,
            Command::SubagentDiff => Self::SubagentDiff,
            _ => return None,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub(crate) enum RunLifecycleReport {
    List(listing::RunListReport),
    Log(logs::LogReport),
    Rerun(RunView),
    Kill(control::KillReport),
    History(history::HistoryReport),
    Status(listing::RunStatusReport),
    Diff(diff::DiffReport),
    SubagentDiff(diff::SubagentDiffReport),
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RunView {
    pub id: String,
    pub engine: String,
    pub model: String,
    pub status: String,
    pub account: String,
    pub session: Option<String>,
    pub started: i64,
    pub ended: Option<i64>,
    pub attempt: u32,
    pub worker_pid: Option<u32>,
    pub supervisor_pid: Option<u32>,
    pub branch: Option<String>,
    pub worktree: Option<PathBuf>,
    pub tag: Option<String>,
    pub prompt: String,
    pub error: Option<String>,
    pub acknowledged: bool,
    pub killed: bool,
    pub children: usize,
}

impl RunView {
    pub(crate) fn from_record<P: neomax_core::runs::ProcessProbe + ?Sized>(
        run: &RunRecord,
        probe: &P,
    ) -> Self {
        Self {
            id: run.id.clone(),
            engine: run.engine.to_string(),
            model: run.model.clone(),
            status: neomax_core::runs::effective_status(run, probe)
                .as_str()
                .to_owned(),
            account: run.account(),
            session: run.session.clone(),
            started: run.started,
            ended: run.ended,
            attempt: run.attempt,
            worker_pid: run.worker_pid,
            supervisor_pid: run.supervisor_pid,
            branch: run.branch.clone(),
            worktree: run.worktree.clone(),
            tag: run.tag.clone(),
            prompt: run.prompt.clone(),
            error: run.error_detail.clone(),
            acknowledged: run.is_acknowledged(),
            killed: run.killed,
            children: run.children.len(),
        }
    }
}

pub(crate) fn execute(
    command: RunLifecycleCommand,
    context: &RuntimeContext,
    args: &[String],
    executor: Option<&dyn RunExecutor>,
    retry_selector: Option<&dyn RetryAccountSelector>,
) -> Result<RunLifecycleReport> {
    let process = SystemProcessControl;
    execute_with_process(command, context, args, &process, executor, retry_selector)
}

pub(crate) fn execute_native(
    command: RunLifecycleCommand,
    context: &RuntimeContext,
    args: &[String],
) -> Result<()> {
    let report = execute_native_report(command, context, args)?;
    if options::json(args) {
        crate::output::json(&report)
    } else {
        println!("{}", render::text(&report));
        Ok(())
    }
}

pub(crate) fn execute_native_report(
    command: RunLifecycleCommand,
    context: &RuntimeContext,
    args: &[String],
) -> Result<RunLifecycleReport> {
    let provider_runtime = context.provider_runtime()?;
    let providers = provider_runtime.registry();
    let runs = RunStore::new(&context.paths.runs);
    let usage = UsageCacheStore::new(&context.paths.usage);
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let probe = SystemProcessProbe;
    let live_work = RunLiveWorkSource::with_system(&runs, &probe);
    let inventory = AccountInventory {
        providers,
        quota: &usage,
        controls: &controls,
        live_work: &live_work,
    };
    let scope = WorkerScope::all();
    let selector = InventoryRetrySelector {
        inventory: &inventory,
        scope: &scope,
        now: Utc::now(),
        policy: SelectionPolicy::from_settings(&context.settings),
    };
    let executor = NativeRunExecutor {
        paths: context.paths.clone(),
        settings: context.settings.clone(),
        provider_runtime: provider_runtime.clone(),
    };
    execute(command, context, args, Some(&executor), Some(&selector))
}

pub(crate) fn execute_with_process(
    command: RunLifecycleCommand,
    context: &RuntimeContext,
    args: &[String],
    process: &dyn ProcessControl,
    executor: Option<&dyn RunExecutor>,
    retry_selector: Option<&dyn RetryAccountSelector>,
) -> Result<RunLifecycleReport> {
    match command {
        RunLifecycleCommand::List => listing::list(context, args, process),
        RunLifecycleCommand::Log => logs::log(context, args),
        RunLifecycleCommand::Resume => {
            control::rerun(context, args, true, process, executor, retry_selector)
        }
        RunLifecycleCommand::Retry => {
            control::rerun(context, args, false, process, executor, retry_selector)
        }
        RunLifecycleCommand::Kill => control::kill(context, args, process),
        RunLifecycleCommand::History => history::history(context, args),
        RunLifecycleCommand::Status => listing::status(context, args, process),
        RunLifecycleCommand::Diff => diff::run_diff(context, args),
        RunLifecycleCommand::SubagentDiff => diff::run_subagent_diff(context, args),
    }
}
