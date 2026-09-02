use anyhow::Result;
use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::providers::catalog::CLAUDE_OPUS_MODEL_1M;

use crate::adapters::ProviderAdapter;
use crate::context::RuntimeContext;

use super::environment;
use super::models;
use super::parser;
use super::scope;
use super::types::{AdapterPlan, LaunchMode, LaunchOptions, LaunchPlan};
use super::{invocation_name, options};

pub(crate) fn build(
    launcher: Launcher,
    options: LaunchOptions,
    context: &RuntimeContext,
) -> Result<LaunchPlan> {
    options::validate(launcher, &options)?;
    let (mode, pinned_engine) = match launcher {
        Launcher::Universal if options.solo => (LaunchMode::Solo, None),
        Launcher::ProviderOrchestrator(engine) if options.solo => (LaunchMode::Solo, Some(engine)),
        Launcher::Universal => (LaunchMode::Dynamic, None),
        Launcher::ProviderOrchestrator(engine) => (LaunchMode::ProviderPinned, Some(engine)),
        Launcher::AccountHelper(engine) => (LaunchMode::AccountHelper, Some(engine)),
    };
    let orchestrator = options.engine.or(pinned_engine);

    let worker_scope = scope::effective(launcher, options.worker_scope.clone())?;
    let worker_engines = worker_scope
        .engines()
        .map(|engine| engine.to_string())
        .collect::<Vec<_>>();
    let overrides = context.model_overrides()?;
    let mut provider_models = options.provider_models.clone();
    if options.opus {
        provider_models.insert(Engine::Claude, CLAUDE_OPUS_MODEL_1M.into());
    }
    let models = models::effective_models(
        &overrides,
        &provider_models,
        options.model.as_deref(),
        orchestrator,
    )?;
    let adapters = ProviderAdapter::all()
        .into_iter()
        .filter(|adapter| {
            worker_scope.contains(adapter.engine) || Some(adapter.engine) == orchestrator
        })
        .map(|adapter| {
            let engine = adapter.engine;
            let role = if options.solo && (orchestrator.is_none() || Some(engine) == orchestrator) {
                "solo"
            } else if Some(engine) == orchestrator {
                "orchestrator"
            } else {
                "worker-pool"
            };
            AdapterPlan {
                provider: adapter.label.to_owned(),
                executable: adapter.executable.to_owned(),
                role: role.into(),
                execution: "not-run".into(),
                environment: environment::environment_plan(
                    context,
                    engine,
                    role,
                    models
                        .get(&engine.to_string())
                        .map(|model| model.model.as_str()),
                ),
            }
        })
        .collect();
    let operation = options.helper_command;
    let operation_args = options.helper_args;
    let initial_task = if mode == LaunchMode::AccountHelper {
        None
    } else {
        parser::join_positionals(options.positionals)
    };
    Ok(LaunchPlan {
        invocation: invocation_name(launcher).into(),
        mode,
        orchestrator: orchestrator.map(|engine| engine.to_string()),
        worker_engines,
        routing: options.routing,
        account: options.account,
        operation,
        operation_args,
        initial_task,
        goal: options.goal,
        base: options.base,
        run_id: options.run_id,
        tag: options.tag,
        session_id: options.session_id,
        max_turns: options.max_turns,
        priority: options.priority,
        effort: options.effort,
        wall_min: options.wall_min,
        stall_min: options.stall_min,
        no_failover: options.no_failover,
        no_worktree: options.no_worktree,
        plan_mode: options.plan_mode,
        open_pull_request: options.open_pull_request,
        brief: options.brief,
        ultra: options.ultra,
        opus: options.opus,
        resume: options.resume,
        dedicated: options.dedicated,
        detach: options.detach,
        foreground: options.foreground,
        worker_dispatch: options.worker_dispatch,
        solo: options.solo,
        models,
        adapters,
        dry_run: true,
        provider_execution: "disabled".into(),
    })
}
