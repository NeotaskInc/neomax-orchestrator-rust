use std::fs::File;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::Arc;

use crate::Result;
use crate::providers::{Provider, ProviderCommand, scrub_provider_environment};
use crate::runs::{RunRecord, RunStatus};
use crate::runtime;

use super::classify::{apply_outcome, classify_attempt};
use super::logs::AttemptLogFiles;
use super::monitor;
use super::prepare::PreparedAttempt;
use super::signals::SignalGuard;
use super::types::{AttemptOutcome, SupervisorConfig, SupervisorDirective};
use crate::io::process_group::{self, ProcessControl, SystemProcessControl};

pub struct AttemptSupervisor<'a> {
    provider: &'a dyn Provider,
    config: SupervisorConfig,
    process_control: Arc<dyn ProcessControl>,
}

impl<'a> AttemptSupervisor<'a> {
    pub fn new(provider: &'a dyn Provider, config: SupervisorConfig) -> Self {
        Self::with_process_control(provider, config, Arc::new(SystemProcessControl))
    }

    pub(crate) fn with_process_control(
        provider: &'a dyn Provider,
        config: SupervisorConfig,
        process_control: Arc<dyn ProcessControl>,
    ) -> Self {
        Self {
            provider,
            config,
            process_control,
        }
    }

    pub fn run<F>(
        &self,
        prepared: PreparedAttempt,
        run: &mut RunRecord,
        logs_directory: &Path,
        resumed: bool,
        on_spawn: F,
    ) -> Result<AttemptOutcome>
    where
        F: FnMut(&RunRecord) -> Result<()>,
    {
        self.run_monitored(prepared, run, logs_directory, resumed, on_spawn, || {
            Ok(SupervisorDirective::Continue)
        })
    }

    pub fn run_monitored<F, M>(
        &self,
        prepared: PreparedAttempt,
        run: &mut RunRecord,
        logs_directory: &Path,
        resumed: bool,
        mut on_spawn: F,
        mut monitor: M,
    ) -> Result<AttemptOutcome>
    where
        F: FnMut(&RunRecord) -> Result<()>,
        M: FnMut() -> Result<SupervisorDirective>,
    {
        let mut signals = SignalGuard::install()?;
        let logs = AttemptLogFiles::open(logs_directory, &run.id, run.attempt)?;
        run.log = Some(logs.log_path.clone());

        let mut command = prepared.command().clone();
        if let Some(bootstrap) = prepared.bootstrap_command().cloned() {
            let (mut child, containment) = spawn(&bootstrap, &logs.stdout, &logs.stderr)?;
            let monitored = match monitor::wait(
                &mut child,
                &containment,
                &logs,
                &self.config,
                self.process_control.as_ref(),
                || {
                    if signals.poll().is_some() {
                        return Ok(SupervisorDirective::Abort);
                    }
                    monitor()
                },
            ) {
                Ok(value) => value,
                Err(error) => {
                    let _ = logs.sync();
                    return Err(error);
                }
            };
            self.process_control
                .terminate_residual(&containment, self.config.terminate_grace);
            let bootstrap_outcome = self.collect_outcome(&logs, monitored)?;
            if bootstrap_outcome.status != RunStatus::Done {
                apply_outcome(run, &bootstrap_outcome, resumed);
                if let Some(signal) = signals.last_signal() {
                    run.interrupt_signal = Some(signal.number());
                }
                return Ok(bootstrap_outcome);
            }
            let session = bootstrap_outcome.parsed.session_id.clone().ok_or_else(|| {
                crate::Error::Provider {
                    provider: self.provider.engine().to_string(),
                    message: "bootstrap completed without a resumable session id".into(),
                }
            })?;
            run.session = Some(session.clone());
            command = prepared.resumed_orchestrator_command(self.provider, &session)?;
        }

        let (mut child, containment) = spawn(&command, &logs.stdout, &logs.stderr)?;
        let worker_pid = child.id();
        run.worker_pid = Some(worker_pid);
        if let Err(error) = on_spawn(run) {
            self.process_control
                .terminate(&mut child, &containment, self.config.terminate_grace);
            self.process_control
                .terminate_residual(&containment, self.config.terminate_grace);
            let _ = logs.sync();
            return Err(error);
        }

        let monitored = match monitor::wait(
            &mut child,
            &containment,
            &logs,
            &self.config,
            self.process_control.as_ref(),
            || {
                if signals.poll().is_some() {
                    return Ok(SupervisorDirective::Abort);
                }
                monitor()
            },
        ) {
            Ok(value) => value,
            Err(error) => {
                let _ = logs.sync();
                return Err(error);
            }
        };
        self.process_control
            .terminate_residual(&containment, self.config.terminate_grace);
        let outcome = self.collect_outcome(&logs, monitored)?;
        apply_outcome(run, &outcome, resumed);
        if let Some(signal) = signals.last_signal() {
            run.interrupt_signal = Some(signal.number());
        }
        Ok(outcome)
    }

    fn collect_outcome(
        &self,
        logs: &AttemptLogFiles,
        monitored: monitor::MonitorOutcome,
    ) -> Result<AttemptOutcome> {
        logs.sync()?;
        let bytes = logs.output()?;
        let mut parsed = self.provider.parse_events(&bytes)?;
        if let Some(SupervisorDirective::Rotate(rotation)) = monitored.directive {
            parsed.rate_limited = true;
            parsed.resets_at = rotation.resets_at.or(parsed.resets_at);
            parsed.limit_window = rotation.limit_window.or(parsed.limit_window);
            parsed.errors.push(rotation.reason);
        }
        let stderr_tail = logs.stderr_tail(4_000)?;
        let exit_code = monitored.exit_status.code();
        let status = classify_attempt(exit_code, &parsed, monitored.killed_for, &stderr_tail);
        Ok(AttemptOutcome {
            status,
            exit_code,
            parsed,
            stderr_tail,
            log_path: logs.log_path.clone(),
            stderr_path: logs.stderr_path.clone(),
        })
    }
}

fn spawn(
    command: &ProviderCommand,
    stdout: &File,
    stderr: &File,
) -> Result<(Child, process_group::ChildContainment)> {
    let mut process = runtime::process_command(&command.program, &command.args, &command.cwd)?;
    scrub_provider_environment(&mut process);
    let stdin = if command.inherit_stdin {
        Stdio::inherit()
    } else {
        Stdio::null()
    };
    process
        .current_dir(&command.cwd)
        .stdin(stdin)
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?));
    for key in &command.env_remove {
        process.env_remove(key);
    }
    for (key, value) in &command.env {
        process.env(key, value);
    }
    process_group::spawn_managed(&mut process)
}
