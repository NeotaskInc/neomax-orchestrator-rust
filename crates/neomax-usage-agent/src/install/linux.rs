#[cfg(target_os = "linux")]
use std::fs;

#[cfg(any(target_os = "linux", all(test, unix)))]
use anyhow::Context;
use anyhow::Result;
use neomax_core::io::path_to_string;

use crate::config::AgentConfig;
#[cfg(target_os = "linux")]
use crate::install::runner::SystemRunner;
#[cfg(any(target_os = "linux", all(test, unix)))]
use crate::install::runner::{COMMAND_TIMEOUT, CommandRunner, success};
use crate::install::unix_environment_values;
#[cfg(any(target_os = "linux", all(test, unix)))]
use crate::install::write_service_artifact;
#[cfg(any(target_os = "linux", all(test, unix)))]
use crate::install::{ServiceReport, ServiceState, ensure_parent, validate_service_paths};

#[cfg(any(target_os = "linux", all(test, unix)))]
const UNIT_NAME: &str = "neomax-usage-agent.service";

#[cfg(target_os = "linux")]
pub(crate) fn install(config: &AgentConfig) -> Result<ServiceReport> {
    install_with(config, &SystemRunner)
}

#[cfg(any(target_os = "linux", all(test, unix)))]
pub(crate) fn install_with(
    config: &AgentConfig,
    runner: &dyn CommandRunner,
) -> Result<ServiceReport> {
    validate_service_paths(config)?;
    let path = &config.paths.systemd_unit;
    let path_text = path_to_string("systemd service path", path)?;
    ensure_parent(path)?;
    write_service_artifact(path, &unit(config)?)
        .with_context(|| format!("write systemd service {}", path.display()))?;
    let reload = runner.run("systemctl", &["--user", "daemon-reload"], COMMAND_TIMEOUT);
    let start = runner.run(
        "systemctl",
        &["--user", "enable", "--now", UNIT_NAME],
        COMMAND_TIMEOUT,
    );
    let state = if reload.as_ref().is_ok_and(success) && start.as_ref().is_ok_and(success) {
        ServiceState::Active
    } else {
        ServiceState::Unknown
    };
    Ok(ServiceReport {
        platform: "linux".into(),
        path: path_text,
        state,
        detail: if state == ServiceState::Active {
            "installed and started"
        } else {
            "wrote unit but systemctl could not start it"
        }
        .into(),
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn uninstall(config: &AgentConfig) -> Result<ServiceReport> {
    uninstall_with(config, &SystemRunner)
}

#[cfg(target_os = "linux")]
pub(crate) fn uninstall_with(
    config: &AgentConfig,
    runner: &dyn CommandRunner,
) -> Result<ServiceReport> {
    validate_service_paths(config)?;
    let path = &config.paths.systemd_unit;
    let path_text = path_to_string("systemd service path", path)?;
    let _ = runner.run(
        "systemctl",
        &["--user", "disable", "--now", UNIT_NAME],
        COMMAND_TIMEOUT,
    );
    if path.exists() {
        fs::remove_file(path)?;
    }
    let _ = runner.run("systemctl", &["--user", "daemon-reload"], COMMAND_TIMEOUT);
    Ok(ServiceReport {
        platform: "linux".into(),
        path: path_text,
        state: ServiceState::Inactive,
        detail: "uninstalled".into(),
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn status(config: &AgentConfig) -> Result<ServiceReport> {
    status_with(config, &SystemRunner)
}

#[cfg(target_os = "linux")]
pub(crate) fn status_with(
    config: &AgentConfig,
    runner: &dyn CommandRunner,
) -> Result<ServiceReport> {
    validate_service_paths(config)?;
    let path_text = path_to_string("systemd service path", &config.paths.systemd_unit)?;
    let output = runner.run(
        "systemctl",
        &["--user", "is-active", UNIT_NAME],
        COMMAND_TIMEOUT,
    );
    let active = output.as_ref().is_ok_and(success);
    Ok(ServiceReport {
        platform: "linux".into(),
        path: path_text,
        state: if active {
            ServiceState::Active
        } else {
            ServiceState::Inactive
        },
        detail: if active {
            "active"
        } else {
            "inactive; run install"
        }
        .into(),
    })
}

fn unit(config: &AgentConfig) -> Result<String> {
    crate::install::validate_service_paths(config)?;
    let state = path_to_string("usage-agent state path", &config.paths.state.state)?;
    let _home = path_to_string("usage-agent home path", &config.paths.home)?;
    let _cli = path_to_string("Neomax CLI path", &config.neomax_cli)?;
    let executable = path_to_string("usage-agent executable path", &config.executable)?;
    let environment = unix_environment_values(config)?
        .into_iter()
        .map(|(key, value)| format!("Environment=\"{key}={}\"\n", systemd_escape(&value)))
        .collect::<String>();
    Ok(format!(
        "[Unit]\nDescription=Neomax local usage collector\nAfter=default.target\n\n[Service]\nExecStart={} run\nWorkingDirectory={}\nRestart=always\nRestartSec=10\n{}\n[Install]\nWantedBy=default.target\n",
        systemd_word(&executable),
        systemd_word(&state),
        environment,
    ))
}

fn systemd_word(value: &str) -> String {
    format!("\"{}\"", systemd_escape(value))
}

fn systemd_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
}

#[cfg(test)]
mod tests;
