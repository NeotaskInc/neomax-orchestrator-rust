use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use neomax_core::Engine;
use neomax_core::WorkerScope;
use neomax_core::accounts::{AccountControlStore, AccountInventory};
use neomax_core::orchestration::commands::{Command, Launcher};
use neomax_core::orchestration::registry::OrchestratorStore;
use neomax_core::orchestration::selection::{
    NeomaxSelectionRequest, OrchestratorPolicy, ProviderSelectionRequest, SelectionStateStore,
    choose_neomax_orchestrator, choose_provider_orchestrator, engine_priority,
};
use neomax_core::providers::ProviderRuntime;
use neomax_core::runs::{RunLiveWorkSource, RunStore, SystemProcessProbe};
use neomax_core::usage::UsageCacheStore;
use serde::Serialize;

use crate::context::RuntimeContext;
use crate::error;
use crate::output;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PickOptions {
    engine: Option<Engine>,
    priority: Option<String>,
    cwd: Option<PathBuf>,
    resume: bool,
    dedicated: bool,
    record: bool,
    json: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PickOrchestratorResult {
    engine: Engine,
    profile: PathBuf,
    account: String,
    pressure: Option<f64>,
    live: u32,
}

#[derive(Debug, Clone, Serialize)]
struct CoLocationResult {
    engine: Engine,
    directory: PathBuf,
    count: usize,
    session: Option<String>,
}

pub(crate) fn execute(
    launcher: Launcher,
    command: Command,
    args: &[String],
    context: &RuntimeContext,
) -> anyhow::Result<()> {
    match command {
        Command::PickOrchestrator => pick_orchestrator(launcher, args, context),
        Command::PickNeomax => pick_neomax(args, context),
        Command::OrchestratorOn => orchestrator_on(args, context),
        _ => bail!("unsupported orchestrator selection command {command:?}"),
    }
}

fn pick_orchestrator(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    let options = error::usage(parse_pick_options(args, false))?;
    let engine = options
        .engine
        .or_else(|| launcher_engine(launcher))
        .unwrap_or(Engine::Claude);
    let runtime = context.provider_runtime()?;
    let now = timestamp(context.now);
    let accounts = account_snapshots(context, &runtime, WorkerScope::only(engine), now)?;
    let orchestrators = OrchestratorStore::new(&context.paths.orchestrators)
        .all(&SystemProcessProbe, context.now)?;
    let current_session = current_session();
    let policy = OrchestratorPolicy::default();
    let selected = choose_provider_orchestrator(&ProviderSelectionRequest {
        accounts: &accounts,
        orchestrators: &orchestrators,
        engine,
        dedicated: options.dedicated,
        current_session: current_session.as_deref(),
        now,
        policy: &policy,
    });
    let Some(selected) = selected else {
        if options.json {
            return output::json(&Option::<PickOrchestratorResult>::None);
        }
        return Ok(());
    };
    let result = PickOrchestratorResult {
        engine: selected.engine,
        profile: selected.profile,
        account: selected.account,
        pressure: selected
            .five_hour_percent
            .into_iter()
            .chain(selected.weekly_percent)
            .reduce(f64::max),
        live: selected.live_workers,
    };
    if options.json {
        output::json(&result)
    } else {
        println!("{}", result.profile.display());
        Ok(())
    }
}

fn pick_neomax(args: &[String], context: &RuntimeContext) -> Result<()> {
    pick_neomax_with_explanation(args, context, false)
}

fn pick_neomax_with_explanation(
    args: &[String],
    context: &RuntimeContext,
    explanation: bool,
) -> Result<()> {
    let options = error::usage(parse_pick_options(args, true))?;
    let runtime = context.provider_runtime()?;
    let cwd = options
        .cwd
        .as_deref()
        .map(|path| context.resolve_path(&path.to_string_lossy()))
        .unwrap_or_else(|| context.cwd.clone());
    let configured_priority = options
        .priority
        .as_deref()
        .map(str::to_owned)
        .or_else(|| std::env::var("NEOMAX_ENGINE_PRIORITY").ok());
    let priority = error::usage(engine_priority(configured_priority.as_deref()))?;
    let now = timestamp(context.now);
    let accounts = account_snapshots(context, &runtime, WorkerScope::all(), now)?;
    let orchestrators = OrchestratorStore::new(&context.paths.orchestrators)
        .all(&SystemProcessProbe, context.now)?;
    let state = SelectionStateStore::new(&context.paths.orchestrator_selection);
    let previous_engine = state.previous_engine(&cwd);
    let current_session = current_session();
    let choice = choose_neomax_orchestrator(NeomaxSelectionRequest {
        accounts: &accounts,
        orchestrators: &orchestrators,
        priority: &priority,
        forced_engine: options.engine,
        cwd: cwd.clone(),
        resume: options.resume,
        dedicated: options.dedicated,
        previous_engine,
        current_session: current_session.as_deref(),
        now,
        policy: &OrchestratorPolicy::default(),
    })
    .ok_or_else(|| {
        anyhow::anyhow!("no eligible authenticated provider orchestrator is available")
    })?;
    if options.record {
        state.record(&cwd, choice.engine, context.now)?;
    }
    if options.json {
        return output::json(&choice);
    }
    if explanation {
        println!(
            "neomax -> {} orchestrator workers={}",
            choice.engine,
            choice
                .worker_engines
                .iter()
                .map(|engine| engine.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        println!("neomax -> {}", choice.reason);
    } else {
        println!("{}", choice.engine);
    }
    Ok(())
}

fn orchestrator_on(args: &[String], context: &RuntimeContext) -> Result<()> {
    let options = error::usage(parse_pick_options(args, false))?;
    let current_session = current_session();
    let Some(directory) = options.cwd else {
        if options.json {
            return output::json(&CoLocationResult {
                engine: options.engine.unwrap_or(Engine::Claude),
                directory: PathBuf::new(),
                count: 0,
                session: current_session.clone(),
            });
        }
        println!("0");
        return Ok(());
    };
    let engine = options.engine.unwrap_or(Engine::Claude);
    let directory = context.resolve_path(&directory.to_string_lossy());
    let count = OrchestratorStore::new(&context.paths.orchestrators)
        .on_account(
            &directory,
            engine,
            current_session.as_deref(),
            &SystemProcessProbe,
            context.now,
        )?
        .len();
    let result = CoLocationResult {
        engine,
        directory,
        count,
        session: current_session,
    };
    if options.json {
        output::json(&result)
    } else {
        println!("{}", result.count);
        Ok(())
    }
}

pub(crate) fn select(args: &[String], context: &RuntimeContext) -> Result<()> {
    pick_neomax_with_explanation(args, context, true)
}

pub(crate) fn why(args: &[String], context: &RuntimeContext) -> Result<()> {
    pick_neomax_with_explanation(args, context, true)
}

fn account_snapshots(
    context: &RuntimeContext,
    runtime: &ProviderRuntime,
    scope: WorkerScope,
    now: DateTime<Utc>,
) -> Result<Vec<neomax_core::accounts::AccountSnapshot>> {
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let usage = UsageCacheStore::new(&context.paths.usage);
    let runs = RunStore::new(&context.paths.runs);
    let probe = SystemProcessProbe;
    let live_work = RunLiveWorkSource::with_system(&runs, &probe);
    let inventory = AccountInventory {
        providers: runtime.registry(),
        quota: &usage,
        controls: &controls,
        live_work: &live_work,
    };
    inventory.routing_snapshots(&scope, now).map_err(Into::into)
}

fn parse_pick_options(args: &[String], allow_selection: bool) -> Result<PickOptions> {
    let mut options = PickOptions {
        engine: None,
        priority: None,
        cwd: None,
        resume: false,
        dedicated: false,
        record: false,
        json: false,
    };
    let mut index = 0;
    while index < args.len() {
        let current = &args[index];
        if current == "--json" {
            options.json = true;
            index += 1;
            continue;
        }
        if current == "--resume" {
            if !allow_selection {
                bail!("--resume is not valid for this command");
            }
            options.resume = true;
            index += 1;
            continue;
        }
        if current == "--dedicated" || current == "--orchestrator" {
            options.dedicated = true;
            index += 1;
            continue;
        }
        if current == "--record" {
            if !allow_selection {
                bail!("--record is not valid for this command");
            }
            options.record = true;
            index += 1;
            continue;
        }
        let (flag, inline) = current
            .split_once('=')
            .map_or((current.as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        let accepts = matches!(
            flag,
            "--engine" | "--priority" | "--prefer" | "--cwd" | "--dir"
        );
        if accepts {
            if !allow_selection && matches!(flag, "--priority" | "--prefer") {
                bail!("{flag} is not valid for this command");
            }
            let value = if let Some(inline) = inline {
                if inline.is_empty() {
                    bail!("{flag} requires a value");
                }
                inline.to_owned()
            } else {
                let next = args
                    .get(index + 1)
                    .with_context(|| format!("{flag} requires a value"))?;
                index += 1;
                next.clone()
            };
            match flag {
                "--engine" => options.engine = Some(value.parse()?),
                "--priority" | "--prefer" => options.priority = Some(value),
                "--cwd" | "--dir" => options.cwd = Some(PathBuf::from(value)),
                _ => unreachable!(),
            }
            index += 1;
            continue;
        }
        if current.starts_with('-') {
            bail!("unknown orchestrator selection option {current}");
        }
        bail!("unexpected orchestrator selection argument {current}");
    }
    Ok(options)
}

fn launcher_engine(launcher: Launcher) -> Option<Engine> {
    match launcher {
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
        Launcher::Universal => None,
    }
}

fn current_session() -> Option<String> {
    std::env::var("NEOMAX_ORCH_SESSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn timestamp(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap_or_else(Utc::now)
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
