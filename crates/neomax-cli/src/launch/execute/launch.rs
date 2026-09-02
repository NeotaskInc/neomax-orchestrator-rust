use anyhow::{Context, Result, bail};
use chrono::Utc;
use neomax_core::WorkerScope;
use neomax_core::accounts::{AccountControlStore, AccountInventory};
use neomax_core::concurrency::dispatch::{AdmissionRequest, DispatchAdmissionStore};
use neomax_core::orchestration::registry::OrchestratorStore;
use neomax_core::providers::ProviderRegistry;
use neomax_core::runs::{RunLiveWorkSource, RunStore, SystemProcessProbe};
use neomax_core::usage::UsageCacheStore;

use crate::context::RuntimeContext;
use crate::output;

use super::super::handshake;
use super::super::invocation_name;
use super::super::parser::join_positionals;
use super::super::types::LaunchOptions;
use super::account;
use super::claude_profile;
use super::coordinator;
use super::record;
use super::report;
use super::selection;
use super::worktree;

pub(super) fn run(
    launcher: neomax_core::orchestration::commands::Launcher,
    options: LaunchOptions,
    context: &RuntimeContext,
    json_output: bool,
) -> Result<()> {
    let provider_runtime = context.provider_runtime()?;
    run_with_registry(
        launcher,
        options,
        context,
        json_output,
        provider_runtime.registry(),
    )
}

pub(crate) fn run_with_registry(
    launcher: neomax_core::orchestration::commands::Launcher,
    mut options: LaunchOptions,
    context: &RuntimeContext,
    json_output: bool,
    providers: &ProviderRegistry,
) -> Result<()> {
    if matches!(
        launcher,
        neomax_core::orchestration::commands::Launcher::AccountHelper(_)
    ) {
        return account::run(launcher, &options, context, json_output);
    }
    let prompt = join_positionals(options.positionals.clone()).unwrap_or_default();
    if options.resume && options.session_id.is_none() {
        bail!("--resume requires --session-id SESSION");
    }

    let probe = SystemProcessProbe;
    let runs = RunStore::new(&context.paths.runs);
    let worker_id = options.worker_dispatch.then(|| {
        options
            .run_id
            .clone()
            .unwrap_or_else(|| record::next_run_id(&runs, context.now))
    });
    let worker_admission = if let Some(id) = worker_id.as_ref() {
        let store = DispatchAdmissionStore::from_settings(&context.paths.state, &context.settings)?;
        Some(store.reserve(AdmissionRequest::new(
            id.clone(),
            id.clone(),
            options.engine,
        ))?)
    } else {
        None
    };
    let usage = UsageCacheStore::new(&context.paths.usage);
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let live_work = RunLiveWorkSource::with_system(&runs, &probe);
    let inventory = AccountInventory {
        providers,
        quota: &usage,
        controls: &controls,
        live_work: &live_work,
    };
    let mut accounts = inventory.routing_snapshots(&WorkerScope::all(), Utc::now())?;
    claude_profile::prepare_pinned_profile(launcher, &options, context, &mut accounts)?;
    let orchestrator_store = OrchestratorStore::new(&context.paths.orchestrators);
    let orchestrators = orchestrator_store.all(&probe, context.now)?;
    let scope = super::super::effective_worker_scope(launcher, options.worker_scope.clone())?;
    let mut selected = selection::choose_target(
        launcher,
        &options,
        context,
        &accounts,
        &orchestrators,
        &scope,
    )?;
    let engine = selected.engine;
    if options.solo && engine == neomax_core::Engine::Claude {
        let setup = crate::operations::setup_profile(context, Some(&selected.account))?;
        selected.profile = setup.destination_profile;
        selected.account = "solo".into();
        selected.reserved = false;
    }
    super::super::parser::normalize_provider_options(engine, &mut options)?;
    if options.engine.is_none() {
        neomax_core::orchestration::selection::SelectionStateStore::new(
            &context.paths.orchestrator_selection,
        )
        .record(&context.cwd, engine, context.now)?;
    }
    providers
        .get(engine)
        .with_context(|| format!("provider adapter {engine} is not registered"))?;
    let model = selection::selected_model(context, &options, engine)?;
    let worker_models = selection::worker_models(context, &options, engine)?;
    let session = options
        .session_id
        .clone()
        .or_else(|| std::env::var("NEOMAX_ORCH_SESSION").ok())
        .filter(|value| !value.trim().is_empty());
    let id = worker_id
        .or_else(|| options.run_id.clone())
        .unwrap_or_else(|| record::next_run_id(&runs, context.now));
    if let Some(lease) = worker_admission.as_ref() {
        lease.bind(
            engine,
            selected.profile.to_string_lossy().into_owned(),
            session.clone().unwrap_or_else(|| id.clone()),
        )?;
    }
    if options.solo {
        crate::operations::arm_profile(context, &selected.profile, Some(&id))?;
    }
    let mut run = record::new_run(record::NewRunInput {
        launcher,
        id: &id,
        engine,
        model: &model.model,
        prompt: &prompt,
        profile: selected.profile.clone(),
        workdir: context.cwd.clone(),
        goal: options.goal.clone(),
        base: options.base.clone(),
        tag: options.tag.clone(),
        max_turns: options.max_turns,
        session: session.clone(),
        effort: options.effort.clone(),
        wall_min: options.wall_min,
        stall_min: options.stall_min,
        no_failover: options.no_failover,
        no_worktree: options.no_worktree,
        plan_mode: options.plan_mode,
        open_pull_request: options.open_pull_request,
        ultra: options.ultra,
        opus: options.opus,
        brief: options.brief,
        solo: options.solo,
        context,
        scope: &scope,
        launch_role: if options.worker_dispatch {
            neomax_core::agent_tools::LaunchRole::Worker
        } else {
            neomax_core::agent_tools::LaunchRole::Orchestrator
        },
        orchestrator_reserved: options.dedicated || selected.reserved,
        worker_models,
    });
    if options.resume {
        run.resume_session = session.clone();
    }
    if !options.no_worktree
        && (options.worker_dispatch || options.base.is_some() || options.open_pull_request)
    {
        worktree::allocate(context, &mut run, options.base.as_deref())?;
    }
    if !options.worker_dispatch {
        return super::orchestrator::execute(super::orchestrator::Execution {
            launcher,
            options,
            context,
            providers,
            orchestrators: &orchestrator_store,
            selected: &selected,
            model: &model.model,
            scope: &scope,
            run,
            json_output,
        });
    }
    run.supervisor_pid = Some(std::process::id());
    runs.create(&run)?;
    if let Some(path) = handshake::path_from_environment() {
        if let Err(error) = handshake::write(&path, &handshake::LaunchHandshake::started(&id)) {
            record::finish_failed(&runs, &id, context.now, &error.to_string());
            if !options.worker_dispatch {
                let _ = orchestrator_store.unregister(session.as_deref().unwrap_or(&id));
            }
            handshake::write_error(error.to_string());
            return Err(error);
        }
    }

    let finalization = match coordinator::execute(
        providers,
        &context.settings,
        &context.paths,
        &scope,
        &runs,
        &mut run,
    ) {
        Ok(finalization) => finalization,
        Err(error) => {
            record::finish_failed(&runs, &id, context.now, &error.to_string());
            if !options.worker_dispatch {
                let _ = orchestrator_store.unregister(session.as_deref().unwrap_or(&id));
            }
            return Err(error.into());
        }
    };
    drop(worker_admission);
    if !options.worker_dispatch {
        let _ = orchestrator_store.unregister(session.as_deref().unwrap_or(&id));
    }
    let report = report::ExecutionReport {
        invocation: invocation_name(launcher).into(),
        run_id: id,
        status: finalization.status.as_str().into(),
        engine: run.engine.to_string(),
        account: run.account(),
        model: run.model.clone(),
        session,
        log: run
            .log
            .as_ref()
            .map(|value| value.to_string_lossy().into_owned()),
        worker_scope: scope.csv(),
    };
    if json_output {
        output::json(&report)
    } else {
        println!(
            "{} {} run {} ({}) account {} model {}",
            report.invocation,
            report.status,
            report.run_id,
            report.engine,
            report.account,
            report.model
        );
        if let Some(log) = report.log {
            println!("log: {log}");
        }
        Ok(())
    }
}
