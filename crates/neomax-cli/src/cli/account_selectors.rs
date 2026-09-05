use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::providers::catalog::{
    Environment, MapEnvironment, RealFileSystem, discover_profile_snapshots,
    resolve_profile_selector,
};

use crate::context::RuntimeContext;

pub(super) fn normalize(
    launcher: Launcher,
    args: &[String],
    context: &RuntimeContext,
) -> Result<Option<Vec<String>>> {
    let environment =
        MapEnvironment::new(std::env::vars().collect::<std::collections::BTreeMap<_, _>>())
            .with_home(&context.paths.home)
            .with_current_dir(&context.cwd);
    normalize_with_environment(launcher, args, &environment)
}

fn named(value: &str) -> bool {
    value.contains('@') || value.starts_with("alias:")
}

fn normalize_with_environment(
    launcher: Launcher,
    args: &[String],
    environment: &dyn Environment,
) -> Result<Option<Vec<String>>> {
    let end = args
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(args.len());
    let prefix = &args[..end];
    let mut engine = match launcher {
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
        Launcher::Universal => None,
    };
    for (index, arg) in prefix.iter().enumerate() {
        if let Some(value) = arg.strip_prefix("--engine=").or_else(|| {
            (arg == "--engine")
                .then(|| prefix.get(index + 1).map(String::as_str))
                .flatten()
        }) {
            engine = Some(value.parse::<Engine>()?);
        }
    }
    let mut slots = Vec::new();
    for (index, arg) in prefix.iter().enumerate() {
        for flag in ["--account", "--to", "--destination", "--from", "--source"] {
            if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
                if named(value) {
                    slots.push((index, value.to_owned(), Some(flag)));
                }
            } else if index > 0 && prefix[index - 1] == flag && named(arg) {
                slots.push((index, arg.clone(), None));
            }
        }
    }
    let direct_helper = matches!(launcher, Launcher::AccountHelper(_))
        && prefix.first().is_some_and(|arg| named(arg));
    let positional = if matches!(launcher, Launcher::AccountHelper(_))
        && prefix.first().is_some_and(|arg| {
            matches!(
                arg.as_str(),
                "run" | "login" | "logout" | "whoami" | "models"
            )
        }) {
        Some(1)
    } else if matches!(
        launcher,
        Launcher::ProviderOrchestrator(_) | Launcher::AccountHelper(_)
    ) {
        Some(0)
    } else {
        None
    };
    if let Some(index) = positional {
        if let Some(value) = prefix.get(index).filter(|value| named(value)) {
            slots.push((index, value.clone(), None));
        }
    }
    if slots.is_empty() {
        return Ok(None);
    }
    let Some(engine) = engine else {
        bail!("email and alias account selection requires --engine ENGINE");
    };
    let home = environment
        .home_dir()
        .ok_or_else(|| anyhow::anyhow!("account selection requires a home directory"))?;
    let profiles = discover_profile_snapshots(engine, environment, &RealFileSystem)?;
    let mut output = args.to_vec();
    for (index, selector, inline) in slots {
        let profile = resolve_profile_selector(
            engine,
            &profiles,
            &selector,
            &home,
            environment,
            &RealFileSystem,
        )?;
        if profile.account.parse::<u32>().is_err() && profile.account != "orch" {
            bail!(
                "selected profile has no launchable account number; configure it in the provider profile list"
            );
        }
        output[index] = if let Some(flag) = inline {
            format!("{flag}={}", profile.account)
        } else {
            profile.account.clone()
        };
    }
    if direct_helper {
        output.insert(0, "run".into());
    }
    Ok(Some(output))
}

#[cfg(test)]
#[path = "account_selectors_tests.rs"]
mod tests;
