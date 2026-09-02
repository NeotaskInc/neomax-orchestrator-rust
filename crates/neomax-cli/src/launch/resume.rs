use anyhow::{Result, bail};
use neomax_core::orchestration::commands::Launcher;

use crate::context::RuntimeContext;
use crate::operations::{resolve_resume_target, resolve_resume_target_for_engine};

use super::types::LaunchOptions;

pub(crate) fn resolve(
    launcher: Launcher,
    mut options: LaunchOptions,
    context: &RuntimeContext,
) -> Result<LaunchOptions> {
    if !options.resume {
        return Ok(options);
    }
    if matches!(launcher, Launcher::AccountHelper(_)) {
        bail!("--resume is not valid for account helpers");
    }

    let positional_selector = options
        .session_id
        .is_none()
        .then(|| options.positionals.first())
        .flatten()
        .filter(|value| value.as_str() != "--")
        .cloned();
    let selector = options
        .session_id
        .as_deref()
        .or(positional_selector.as_deref());
    let pinned = match launcher {
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
        Launcher::Universal => None,
    };
    let target = if let Some(engine) = pinned {
        resolve_resume_target_for_engine(context, engine, selector)?
    } else {
        resolve_resume_target(context, selector)?
    };
    if let Some(pinned) = pinned.filter(|engine| *engine != target.engine) {
        bail!(
            "session {} belongs to {}, but {} is pinned to {}",
            target.session_id,
            target.engine,
            launcher_name(launcher),
            pinned
        );
    }
    if let Some(engine) = options.engine.filter(|engine| *engine != target.engine) {
        bail!(
            "session {} belongs to {}, which conflicts with --engine {}",
            target.session_id,
            target.engine,
            engine
        );
    }
    if let Some(account) = options
        .account
        .as_deref()
        .filter(|account| !account.eq_ignore_ascii_case(&target.account))
    {
        bail!(
            "session {} belongs to account {}, which conflicts with --account {}",
            target.session_id,
            target.account,
            account
        );
    }

    options.engine = Some(target.engine);
    options.account = Some(target.account);
    options.session_id = Some(target.session_id);
    options.routing = "account".into();
    if positional_selector.is_some() {
        options.positionals.remove(0);
    }
    Ok(options)
}

fn launcher_name(launcher: Launcher) -> &'static str {
    match launcher {
        Launcher::Universal => "neomax",
        Launcher::ProviderOrchestrator(engine) => match engine {
            neomax_core::Engine::Claude => "cmax",
            neomax_core::Engine::Codex => "cdxmax",
            neomax_core::Engine::Opencode => "ocmax",
            neomax_core::Engine::Kimi => "kmax",
            neomax_core::Engine::Grok => "gmax",
        },
        Launcher::AccountHelper(engine) => match engine {
            neomax_core::Engine::Claude => "c",
            neomax_core::Engine::Codex => "cdx",
            neomax_core::Engine::Opencode => "ocx",
            neomax_core::Engine::Kimi => "kmx",
            neomax_core::Engine::Grok => "gmx",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_names_are_stable_for_pin_errors() {
        assert_eq!(launcher_name(Launcher::Universal), "neomax");
        assert_eq!(
            launcher_name(Launcher::ProviderOrchestrator(neomax_core::Engine::Kimi)),
            "kmax"
        );
    }
}
