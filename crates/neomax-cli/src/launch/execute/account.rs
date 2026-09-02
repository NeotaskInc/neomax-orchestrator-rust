use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::process::ExitStatus;

use anyhow::{Context, Result, bail};
use neomax_core::orchestration::commands::Launcher;
use neomax_core::providers::{ProviderProfile, catalog, scrub_provider_environment};
use neomax_core::runtime;
use neomax_core::{Engine, Error};
use serde::Serialize;

use crate::context::RuntimeContext;
use crate::output;

use super::super::invocation_name;
use super::super::types::LaunchOptions;

pub(crate) trait AccountExecutor {
    fn execute(
        &self,
        program: &OsStr,
        args: &[OsString],
        cwd: &std::path::Path,
        environment: &BTreeMap<OsString, OsString>,
    ) -> Result<ExitStatus>;
}

struct ProcessAccountExecutor;

impl AccountExecutor for ProcessAccountExecutor {
    fn execute(
        &self,
        program: &OsStr,
        args: &[OsString],
        cwd: &std::path::Path,
        environment: &BTreeMap<OsString, OsString>,
    ) -> Result<ExitStatus> {
        let mut command = runtime::process_command(program, args, cwd)?;
        scrub_provider_environment(&mut command);
        command.current_dir(cwd);
        for (key, value) in environment {
            command.env(key, value);
        }
        command.status().map_err(Into::into)
    }
}

#[derive(Debug, Serialize)]
struct AccountReport {
    invocation: String,
    provider: String,
    operation: String,
    account: String,
    exit_code: Option<i32>,
    success: bool,
}

pub(crate) fn run(
    launcher: Launcher,
    options: &LaunchOptions,
    context: &RuntimeContext,
    json_output: bool,
) -> Result<()> {
    run_with_executor(
        launcher,
        options,
        context,
        json_output,
        &ProcessAccountExecutor,
    )
}

fn run_with_executor(
    launcher: Launcher,
    options: &LaunchOptions,
    context: &RuntimeContext,
    json_output: bool,
    executor: &dyn AccountExecutor,
) -> Result<()> {
    let engine = match launcher {
        Launcher::AccountHelper(engine) => engine,
        _ => bail!("account helper requires a provider account launcher"),
    };
    let operation = options.helper_command.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "{} requires an account operation",
            invocation_name(launcher)
        )
    })?;
    let provider_runtime = context.provider_runtime()?;
    let providers = provider_runtime.registry();
    let provider = providers
        .get(engine)
        .with_context(|| format!("provider adapter {engine} is not registered"))?;
    let profiles = providers.profiles_for(engine)?;
    let profile = select_profile(&profiles, options.account.as_deref())?;
    let mut args = Vec::with_capacity(options.helper_args.len() + 1);
    args.push(operation.into());
    args.extend(options.helper_args.iter().cloned().map(Into::into));
    let environment = account_environment(engine, &profile);
    let status = executor.execute(provider.binary(), &args, &context.cwd, &environment)?;
    let report = AccountReport {
        invocation: invocation_name(launcher).into(),
        provider: engine.to_string(),
        operation: operation.into(),
        account: profile.account,
        exit_code: status.code(),
        success: status.success(),
    };
    if json_output {
        output::json(&report)?;
    } else {
        println!(
            "{} {} account {} {}",
            report.invocation,
            report.operation,
            report.account,
            if report.success {
                "succeeded"
            } else {
                "failed"
            }
        );
    }
    if report.success {
        Ok(())
    } else {
        bail!(
            "{} exited unsuccessfully{}",
            report.provider,
            report
                .exit_code
                .map_or_else(String::new, |code| format!(" with status {code}"))
        )
    }
}

fn select_profile(
    profiles: &[ProviderProfile],
    requested: Option<&str>,
) -> Result<ProviderProfile> {
    if let Some(requested) = requested {
        return profiles
            .iter()
            .find(|profile| profile.account.eq_ignore_ascii_case(requested))
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("account {requested}")).into());
    }
    profiles
        .first()
        .cloned()
        .ok_or_else(|| Error::NotFound("no provider account profile".into()).into())
}

fn account_environment(engine: Engine, profile: &ProviderProfile) -> BTreeMap<OsString, OsString> {
    let spec = catalog::spec(engine);
    BTreeMap::from([(
        OsString::from(spec.config_env),
        profile.path.clone().into_os_string(),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_case_insensitive_account_names_without_running_a_provider() {
        let profiles = [ProviderProfile {
            engine: Engine::Codex,
            account: "2".into(),
            path: "/tmp/.codex2".into(),
            reserved: false,
        }];
        assert_eq!(select_profile(&profiles, Some("2")).unwrap().account, "2");
        assert!(select_profile(&profiles, Some("missing")).is_err());
        assert_eq!(
            account_environment(Engine::Codex, &profiles[0]).get(OsStr::new("CODEX_HOME")),
            Some(&OsString::from("/tmp/.codex2"))
        );
    }
}
