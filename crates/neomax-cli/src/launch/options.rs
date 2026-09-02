use anyhow::{Result, bail};
use neomax_core::orchestration::commands::Launcher;

use super::parser;
use super::types::LaunchOptions;
use super::{invocation_name, validation};

pub(crate) fn validate(launcher: Launcher, options: &LaunchOptions) -> Result<()> {
    let pinned_engine = match launcher {
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
        Launcher::Universal => None,
    };
    if let (Some(requested), Some(pinned)) = (options.engine, pinned_engine) {
        if requested != pinned {
            bail!(
                "{} is pinned to {pinned}; --engine {requested} is not valid for this launcher",
                invocation_name(launcher)
            );
        }
    }
    let orchestrator = options.engine.or(pinned_engine);
    if let Some(engine) = orchestrator {
        let mut normalized = options.clone();
        parser::normalize_provider_options(engine, &mut normalized)?;
    }
    if options.detach && options.foreground {
        bail!("--detach and --foreground cannot be used together");
    }
    if options.detach && !options.worker_dispatch && !matches!(launcher, Launcher::AccountHelper(_))
    {
        bail!(
            "--detach is only supported for worker dispatch; use --foreground for an orchestrator launch"
        );
    }
    if options.plan_mode && options.goal.is_some() {
        bail!("--plan and --goal cannot be combined");
    }
    if options.plan_mode && !options.worker_dispatch {
        bail!("--plan is only valid for guarded worker dispatch; use neomax dispatch --plan TASK");
    }
    if options.solo {
        validation::validate_solo_options(options)?;
    }
    validation::validate_run_metadata(options)?;
    validation::validate_effective_base(options)?;
    if options.open_pull_request && options.no_worktree {
        bail!("--pr requires an isolated worktree; remove --no-worktree");
    }
    let explicit_worker_dispatch = std::env::var("NEOMAX_ALLOW_WORKER_DISPATCH")
        .ok()
        .as_deref()
        == Some("1");
    if options.worker_dispatch
        && !options.dry_run
        && std::env::var("NEOMAX_WORKER").ok().as_deref() != Some("1")
        && std::env::var("NEOMAX_ROLE").is_err()
        && !explicit_worker_dispatch
    {
        bail!(
            "worker dispatch requires NEOMAX_ROLE or NEOMAX_WORKER; set NEOMAX_ALLOW_WORKER_DISPATCH=1 to explicitly authorize it"
        );
    }
    Ok(())
}
