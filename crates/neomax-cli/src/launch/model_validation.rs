use anyhow::{Result, bail};
use neomax_core::orchestration::commands::Launcher;

use super::types::LaunchOptions;

pub(crate) fn validate(launcher: Launcher, options: &LaunchOptions) -> Result<()> {
    let pinned_engine = match launcher {
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
        Launcher::Universal => None,
    };
    let target = options.engine.or(pinned_engine).or_else(|| {
        let scope = options.worker_scope.as_ref()?;
        let mut engines = scope.engines();
        let engine = engines.next()?;
        engines.next().is_none().then_some(engine)
    });

    if let Some(engine) = target {
        if let Some(model) = options.provider_models.get(&engine) {
            if let Some(generic) = options.model.as_deref() {
                if generic != model {
                    bail!("neomax: --model and --{engine}-model disagree");
                }
            }
        }
    }

    if !options.worker_dispatch {
        return Ok(());
    }

    let Some(target) = target else {
        if let Some(engine) = options.provider_models.keys().next() {
            bail!("neomax: --{engine}-model requires --engine {engine} for direct worker dispatch");
        }
        return Ok(());
    };

    if let Some(engine) = options
        .provider_models
        .keys()
        .copied()
        .find(|engine| *engine != target)
    {
        bail!("neomax: --{engine}-model requires --engine {engine}");
    }
    Ok(())
}
