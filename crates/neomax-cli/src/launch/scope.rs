use anyhow::{Result, bail};
use neomax_core::WorkerScope;
use neomax_core::orchestration::commands::Launcher;
use std::env;

use super::types::LaunchMode;

pub(crate) fn for_launcher(launcher: Launcher, explicit: Option<WorkerScope>) -> WorkerScope {
    let mode = match launcher {
        Launcher::Universal => LaunchMode::Dynamic,
        Launcher::ProviderOrchestrator(_) => LaunchMode::ProviderPinned,
        Launcher::AccountHelper(_) => LaunchMode::AccountHelper,
    };
    match mode {
        LaunchMode::Dynamic | LaunchMode::ProviderPinned | LaunchMode::Solo => {
            explicit.unwrap_or_else(WorkerScope::all)
        }
        LaunchMode::AccountHelper => match launcher {
            Launcher::AccountHelper(engine) => WorkerScope::only(engine),
            _ => unreachable!("account helper scope requires an account launcher"),
        },
    }
}

pub(crate) fn effective(launcher: Launcher, explicit: Option<WorkerScope>) -> Result<WorkerScope> {
    let requested = for_launcher(launcher, explicit);
    if matches!(launcher, Launcher::AccountHelper(_)) {
        return Ok(requested);
    }
    let Some(raw) = env::var_os("NEOMAX_FLEET") else {
        return Ok(requested);
    };
    let inherited = raw
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("NEOMAX_FLEET must be valid UTF-8"))?
        .parse::<WorkerScope>()
        .map_err(|error| anyhow::anyhow!("invalid NEOMAX_FLEET: {error}"))?;
    let effective = requested.intersection(&inherited);
    if effective.is_empty() {
        bail!(
            "worker scope is empty after applying inherited NEOMAX_FLEET={}",
            raw.to_string_lossy()
        );
    }
    Ok(effective)
}
