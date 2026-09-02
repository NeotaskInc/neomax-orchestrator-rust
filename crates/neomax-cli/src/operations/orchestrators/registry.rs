use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use neomax_core::Engine;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::commands::{Command, Launcher};
use neomax_core::orchestration::registry::{OrchestratorRegistration, OrchestratorStore};
use neomax_core::providers::catalog;
use neomax_core::settings::ModelOverrides;
use serde::Serialize;

use crate::context::RuntimeContext;
use crate::output;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegisterOptions {
    session: Option<String>,
    pid: Option<u32>,
    engine: Option<Engine>,
    account: Option<String>,
    directory: Option<PathBuf>,
    model: Option<String>,
    reserved: bool,
    json: bool,
}

#[derive(Debug, Serialize)]
struct RegistrationResult {
    command: &'static str,
    session: String,
    registered: bool,
    engine: Option<Engine>,
    account: Option<String>,
    directory: Option<PathBuf>,
}

pub(crate) fn execute(
    launcher: Launcher,
    command: Command,
    args: &[String],
    context: &RuntimeContext,
) -> Result<()> {
    match command {
        Command::OrchestratorRegister => register(launcher, args, context),
        Command::OrchestratorUnregister => unregister(args, context),
        _ => bail!("unsupported orchestrator registry command {command:?}"),
    }
}

fn register(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    let options = parse_options(args)?;
    let engine = options
        .engine
        .or_else(|| launcher_engine(launcher))
        .or_else(|| env_engine("NEOMAX_ROLE"))
        .ok_or_else(|| anyhow::anyhow!("orch-register requires --engine or NEOMAX_ROLE"))?;
    let session = options
        .session
        .or_else(|| std::env::var("NEOMAX_ORCH_SESSION").ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("orch-register requires --session or NEOMAX_ORCH_SESSION")
        })?;
    let pid = options
        .pid
        .or_else(|| parse_pid_env("NEOMAX_ORCH_PID"))
        .or_else(|| Some(std::process::id()));
    let runtime = context.provider_runtime()?;
    let profile = resolve_profile(
        &runtime,
        engine,
        options.directory.as_deref(),
        options.account.as_deref(),
        &context.paths.home,
    )?;
    let model_environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let model_overrides = context.model_overrides()?;
    let model = registration_model(
        engine,
        options.model.as_deref(),
        &model_overrides,
        &model_environment,
    )?;
    let reserved = options.reserved
        || std::env::var("NEOMAX_ORCH_RESERVED").ok().as_deref() == Some("1")
        || profile.as_ref().is_some_and(|value| value.reserved);
    let profile = profile.ok_or_else(|| {
        anyhow::anyhow!(
            "orch-register could not resolve an authenticated {engine} profile; pass --dir or --account"
        )
    })?;
    let store = OrchestratorStore::new(&context.paths.orchestrators);
    let record = store.register(OrchestratorRegistration {
        session: session.clone(),
        pid,
        engine,
        account: profile.account.parse().ok(),
        account_dir: profile
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned(),
        project: context.project_for_cwd(),
        branch_prefix: None,
        cwd: std::env::var_os("NEOMAX_PROJECT_ROOT")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| context.cwd.clone()),
        model,
        reserved,
        now: context.now,
    })?;
    let result = RegistrationResult {
        command: "orch-register",
        session: record.session.clone(),
        registered: true,
        engine: Some(record.engine),
        account: Some(record.account_dir.clone()),
        directory: Some(profile.path),
    };
    if options.json {
        output::json(&result)
    } else {
        println!(
            "registered orchestrator {} engine={} account={} pid={}",
            result.session,
            result
                .engine
                .map_or_else(|| "-".into(), |value| value.to_string()),
            result.account.as_deref().unwrap_or("-"),
            pid.map_or_else(|| "-".into(), |value| value.to_string())
        );
        Ok(())
    }
}

fn unregister(args: &[String], context: &RuntimeContext) -> Result<()> {
    let options = parse_options(args)?;
    let session = options
        .session
        .or_else(|| std::env::var("NEOMAX_ORCH_SESSION").ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("orch-unregister requires --session or NEOMAX_ORCH_SESSION")
        })?;
    let removed = OrchestratorStore::new(&context.paths.orchestrators).unregister(&session)?;
    let result = RegistrationResult {
        command: "orch-unregister",
        session,
        registered: !removed,
        engine: None,
        account: None,
        directory: None,
    };
    if options.json {
        output::json(&result)
    } else {
        println!(
            "{} orchestrator {}",
            if removed {
                "unregistered"
            } else {
                "not registered"
            },
            result.session
        );
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ProfileTarget {
    account: String,
    path: PathBuf,
    reserved: bool,
}

fn resolve_profile(
    runtime: &neomax_core::providers::ProviderRuntime,
    engine: Engine,
    directory: Option<&Path>,
    account: Option<&str>,
    home: &Path,
) -> Result<Option<ProfileTarget>> {
    if is_rooted_but_not_absolute(home) {
        bail!(
            "orchestrator profile home must not be rooted without an absolute prefix: {}",
            home.display()
        );
    }
    let profiles = runtime.registry().profiles_for(engine)?;
    let configured_path = directory
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os(catalog::spec(engine).config_env).map(PathBuf::from));
    if let Some(path) = configured_path.as_deref() {
        if is_rooted_but_not_absolute(path) {
            bail!(
                "orchestrator profile path must not be rooted without an absolute prefix: {}",
                path.display()
            );
        }
    }
    let configured = configured_path
        .map(|path| absolute_path(path, home))
        .transpose()?;
    let requested_account = account.map(str::to_ascii_lowercase);
    let target = profiles.into_iter().find(|profile| {
        if is_rooted_but_not_absolute(&profile.path) {
            return false;
        }
        if !runtime.registry().managed_pool_eligible(profile) {
            return false;
        }
        let path_matches = configured.as_deref().is_some_and(|path| {
            absolute_path(profile.path.clone(), home).ok().as_deref() == Some(path)
        });
        let account_matches = requested_account
            .as_deref()
            .is_some_and(|value| profile.account.eq_ignore_ascii_case(value));
        let reserved_matches = requested_account.as_deref() == Some("orch") && profile.reserved;
        (configured.is_some() && path_matches)
            || (requested_account.is_some() && (account_matches || reserved_matches))
            || (configured.is_none() && requested_account.is_none() && profile.reserved)
            || (configured.is_none() && requested_account.is_none() && profile.account == "1")
    });
    Ok(target.map(|profile| ProfileTarget {
        account: profile.account,
        path: profile.path,
        reserved: profile.reserved,
    }))
}

fn parse_options(args: &[String]) -> Result<RegisterOptions> {
    let mut options = RegisterOptions {
        session: None,
        pid: None,
        engine: None,
        account: None,
        directory: None,
        model: None,
        reserved: false,
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
        if current == "--reserved" || current == "--orchestrator" {
            options.reserved = true;
            index += 1;
            continue;
        }
        let (flag, inline) = current
            .split_once('=')
            .map_or((current.as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        let value = |name: &str, index: &mut usize| -> Result<String> {
            if let Some(inline) = inline.filter(|_| flag == name) {
                if inline.is_empty() {
                    bail!("{name} requires a value");
                }
                return Ok(inline.to_owned());
            }
            let next = args
                .get(*index + 1)
                .with_context(|| format!("{name} requires a value"))?;
            *index += 1;
            Ok(next.clone())
        };
        match flag {
            "--session" => options.session = Some(value("--session", &mut index)?),
            "--pid" => {
                options.pid = Some(
                    value("--pid", &mut index)?
                        .parse()
                        .context("--pid must be a positive integer")?,
                )
            }
            "--engine" => options.engine = Some(value("--engine", &mut index)?.parse()?),
            "--account" => options.account = Some(value("--account", &mut index)?),
            "--dir" | "--directory" => {
                options.directory = Some(PathBuf::from(value(flag, &mut index)?))
            }
            "--model" => options.model = Some(value("--model", &mut index)?),
            value if value.starts_with('-') => {
                bail!("unknown orchestrator registry option {value}")
            }
            value => bail!("unexpected orchestrator registry argument {value}"),
        }
        index += 1;
    }
    Ok(options)
}

fn registration_model(
    engine: Engine,
    explicit: Option<&str>,
    overrides: &ModelOverrides,
    environment: &BTreeMap<String, String>,
) -> Result<String> {
    Ok(overrides
        .effective_model_with_environment(engine, explicit, environment)?
        .model)
}

fn launcher_engine(launcher: Launcher) -> Option<Engine> {
    match launcher {
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
        Launcher::Universal => None,
    }
}

fn env_engine(key: &str) -> Option<Engine> {
    std::env::var(key).ok()?.parse().ok()
}

fn parse_pid_env(key: &str) -> Option<u32> {
    std::env::var(key).ok()?.parse().ok().filter(|pid| *pid > 0)
}

fn absolute_path(path: PathBuf, home: &Path) -> Result<PathBuf> {
    if is_rooted_but_not_absolute(&path) {
        bail!(
            "orchestrator profile path must not be rooted without an absolute prefix: {}",
            path.display()
        );
    }
    if is_rooted_but_not_absolute(home) {
        bail!(
            "orchestrator profile home must not be rooted without an absolute prefix: {}",
            home.display()
        );
    }
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(home.join(path))
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
