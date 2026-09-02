use std::io::Read;
use std::process::{Child, Command, Stdio};

use anyhow::{Context, Result, bail};
use neomax_core::WorkerScope;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::registry::OrchestratorStore;
use neomax_core::providers::ProviderRegistry;
use neomax_core::providers::scrub_provider_environment;
use neomax_core::providers::{Provider, ProviderCommand};
use neomax_core::runs::RunRecord;
use neomax_core::runtime;

use crate::context::RuntimeContext;

use super::super::invocation_name;
use super::super::types::LaunchOptions;
use super::record;
use super::report::ExecutionReport;

const MAX_BOOTSTRAP_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct Execution<'a> {
    pub(super) launcher: Launcher,
    pub(super) options: LaunchOptions,
    pub(super) context: &'a RuntimeContext,
    pub(super) providers: &'a ProviderRegistry,
    pub(super) orchestrators: &'a OrchestratorStore,
    pub(super) selected: &'a neomax_core::accounts::AccountSnapshot,
    pub(super) model: &'a str,
    pub(super) scope: &'a WorkerScope,
    pub(super) run: RunRecord,
    pub(super) json_output: bool,
}

pub(super) fn execute(input: Execution<'_>) -> Result<()> {
    let Execution {
        launcher,
        options,
        context,
        providers,
        orchestrators,
        selected,
        model,
        scope,
        mut run,
        json_output,
    } = input;
    let provider = providers
        .get(run.engine)
        .with_context(|| format!("provider adapter {} is not registered", run.engine))?;
    // The launcher/supervisor owns the interactive session. Its PID is known
    // before provider spawn, so the child receives a stable identity and the
    // registry reservation can be visible before provider startup code runs.
    // Do not replace this with child.id(): doing so reintroduces a startup
    // window in which a provider can invoke Neomax before registration.
    run.supervisor_pid = Some(std::process::id());
    let process_secret = providers.process_secret_for(&neomax_core::providers::ProviderProfile {
        engine: run.engine,
        account: run.account(),
        path: run.profile.clone(),
        reserved: run
            .extra
            .get("orchestrator_reserved")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_else(|| run.account().eq_ignore_ascii_case("orch")),
    });
    let prepared = neomax_core::runs::execution::prepare_attempt_with_secret(
        provider,
        &run,
        &context.settings,
        &context.paths,
        run.resume_session.as_deref(),
        process_secret,
    )?;
    let mut command = prepared.command().clone();
    if let Some(bootstrap) = prepared.bootstrap_command() {
        let session = bootstrap_session(provider, bootstrap)?;
        run.session = Some(session.clone());
        command = prepared.resumed_orchestrator_command(provider, &session)?;
    }
    let session = run.session.clone().unwrap_or_else(|| run.id.clone());
    if !options.solo {
        if let Err(error) = record::register_orchestrator(
            orchestrators,
            &run,
            selected,
            context,
            model,
            &Some(session.clone()),
            options.dedicated || selected.reserved,
        ) {
            return Err(error.into());
        }
    }
    let mut child = match spawn(&command, json_output) {
        Ok(child) => child,
        Err(error) => {
            if !options.solo {
                let _ = orchestrators.unregister(&session);
            }
            return Err(error);
        }
    };
    let status = match child.wait() {
        Ok(status) => status,
        Err(error) => {
            if !options.solo {
                let _ = orchestrators.unregister(&session);
            }
            return Err(anyhow::Error::new(error)
                .context(format!("could not wait for {} orchestrator", run.engine)));
        }
    };
    if !options.solo {
        let _ = orchestrators.unregister(&session);
    }
    let status_label = if status.success() { "done" } else { "error" };
    let report = ExecutionReport {
        invocation: invocation_name(launcher).into(),
        run_id: run.id.clone(),
        status: status_label.into(),
        engine: run.engine.to_string(),
        account: run.account(),
        model: model.into(),
        session: run.session,
        log: None,
        worker_scope: scope.csv(),
    };
    if json_output {
        crate::output::json(&report)?;
    } else {
        println!(
            "{} {} orchestrator ({}) account {} model {}",
            report.invocation, report.status, report.engine, report.account, report.model
        );
    }
    if status.success() {
        Ok(())
    } else {
        bail!("{} orchestrator exited unsuccessfully", run.engine)
    }
}

fn spawn(command: &ProviderCommand, json_output: bool) -> Result<Child> {
    let mut process = configured_process(command)?;
    process.stdin(if command.inherit_stdin {
        Stdio::inherit()
    } else {
        Stdio::null()
    });
    if json_output {
        process.stdout(Stdio::null()).stderr(Stdio::null());
    } else {
        process.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }
    process
        .spawn()
        .context("could not start orchestrator provider")
}

fn bootstrap_session(provider: &dyn Provider, command: &ProviderCommand) -> Result<String> {
    let mut process = configured_process(command)?;
    process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = process
        .spawn()
        .context("could not start Kimi session bootstrap")?;
    let mut output = Vec::new();
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        bail!("Kimi session bootstrap did not expose stdout");
    };
    if let Err(error) = stdout
        .take((MAX_BOOTSTRAP_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut output)
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(
            anyhow::Error::new(error).context("could not read Kimi session bootstrap output")
        );
    }
    if output.len() > MAX_BOOTSTRAP_OUTPUT_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        bail!("Kimi session bootstrap output exceeded the safety limit");
    }
    let status = child
        .wait()
        .context("could not wait for Kimi session bootstrap")?;
    if !status.success() {
        bail!("Kimi session bootstrap exited unsuccessfully");
    }
    let events = provider
        .parse_events(&output)
        .context("could not parse Kimi session bootstrap output")?;
    if events.rate_limited {
        bail!("Kimi session bootstrap hit a rate limit");
    }
    if events.is_error {
        bail!("Kimi session bootstrap did not create a session");
    }
    events
        .session_id
        .filter(|session| !session.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Kimi session bootstrap did not return a session ID"))
}

fn configured_process(command: &ProviderCommand) -> Result<Command> {
    let mut process = runtime::process_command(&command.program, &command.args, &command.cwd)?;
    scrub_provider_environment(&mut process);
    process.current_dir(&command.cwd);
    for key in &command.env_remove {
        process.env_remove(key);
    }
    for (key, value) in &command.env {
        process.env(key, value);
    }
    Ok(process)
}
