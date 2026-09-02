use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use neomax_core::io::path_to_string;

use crate::config::{AgentConfig, LEGACY_SERVICE_LABEL, SERVICE_LABEL};
use crate::install::runner::{COMMAND_TIMEOUT, CommandRunner, success};
use crate::install::{
    ServiceReport, ServiceState, ensure_parent, unix_environment_values, validate_service_paths,
    write_service_artifact,
};

#[cfg(target_os = "macos")]
use crate::install::runner::SystemRunner;

#[cfg(target_os = "macos")]
pub(crate) fn install(config: &AgentConfig) -> Result<ServiceReport> {
    install_with(config, &SystemRunner)
}

pub(crate) fn install_with(
    config: &AgentConfig,
    runner: &dyn CommandRunner,
) -> Result<ServiceReport> {
    validate_service_paths(config)?;
    let path = &config.paths.launchd_plist;
    let path_text = path_to_string("launchd service path", path)?;
    ensure_parent(path)?;
    std::fs::create_dir_all(&config.paths.state.state)?;
    let content = plist(config)?;
    write_service_artifact(path, &content)
        .with_context(|| format!("write launchd service {}", path.display()))?;
    let uid = current_uid();
    let domain = format!("gui/{}", uid);
    let _ = runner.run(
        "launchctl",
        &["bootout", &domain, SERVICE_LABEL],
        COMMAND_TIMEOUT,
    );
    let output = runner.run(
        "launchctl",
        &["bootstrap", &domain, &path_text],
        COMMAND_TIMEOUT,
    );
    let (state, detail) = match output {
        Ok(output) if success(&output) => (ServiceState::Loaded, "installed and started".into()),
        Ok(_) => (
            ServiceState::Unknown,
            "wrote plist but launchctl could not load it".into(),
        ),
        Err(error) => (
            ServiceState::Unknown,
            format!("wrote plist; launchctl unavailable: {error}"),
        ),
    };
    Ok(ServiceReport {
        platform: "macos".into(),
        path: path_text,
        state,
        detail,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn uninstall(config: &AgentConfig) -> Result<ServiceReport> {
    uninstall_with(config, &SystemRunner)
}

pub(crate) fn uninstall_with(
    config: &AgentConfig,
    runner: &dyn CommandRunner,
) -> Result<ServiceReport> {
    validate_service_paths(config)?;
    let path = &config.paths.launchd_plist;
    let path_text = path_to_string("launchd service path", path)?;
    let uid = current_uid();
    let domain = format!("gui/{uid}");
    let _ = runner.run(
        "launchctl",
        &["bootout", &domain, SERVICE_LABEL],
        COMMAND_TIMEOUT,
    );
    let _ = runner.run(
        "launchctl",
        &["bootout", &domain, LEGACY_SERVICE_LABEL],
        COMMAND_TIMEOUT,
    );
    if path.exists() {
        fs::remove_file(path)?;
    }
    let legacy_path = legacy_plist_path(config);
    if legacy_path.exists() {
        fs::remove_file(legacy_path)?;
    }
    Ok(ServiceReport {
        platform: "macos".into(),
        path: path_text,
        state: ServiceState::Inactive,
        detail: "uninstalled".into(),
    })
}

fn legacy_plist_path(config: &AgentConfig) -> PathBuf {
    config
        .paths
        .launchd_plist
        .with_file_name(format!("{LEGACY_SERVICE_LABEL}.plist"))
}

#[cfg(target_os = "macos")]
pub(crate) fn status(config: &AgentConfig) -> Result<ServiceReport> {
    status_with(config, &SystemRunner)
}

pub(crate) fn status_with(
    config: &AgentConfig,
    runner: &dyn CommandRunner,
) -> Result<ServiceReport> {
    validate_service_paths(config)?;
    let path = &config.paths.launchd_plist;
    let path_text = path_to_string("launchd service path", path)?;
    let output = runner.run("launchctl", &["list"], COMMAND_TIMEOUT);
    let loaded = output.as_ref().is_ok_and(|output| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.contains(SERVICE_LABEL))
    });
    Ok(ServiceReport {
        platform: "macos".into(),
        path: path_text,
        state: if loaded {
            ServiceState::Loaded
        } else {
            ServiceState::Inactive
        },
        detail: if loaded {
            "loaded"
        } else {
            "not loaded; run install"
        }
        .into(),
    })
}

fn plist(config: &AgentConfig) -> Result<String> {
    validate_service_paths(config)?;
    let executable = xml_escape(&path_to_string(
        "usage-agent executable path",
        &config.executable,
    )?);
    let state = xml_escape(&path_to_string(
        "usage-agent state path",
        &config.paths.state.state,
    )?);
    let _home = path_to_string("usage-agent home path", &config.paths.home)?;
    let _cli = path_to_string("Neomax CLI path", &config.neomax_cli)?;
    let environment = unix_environment_values(config)?
        .into_iter()
        .map(|(key, value)| {
            format!(
                "  <key>{}</key><string>{}</string>\n",
                xml_escape(&key),
                xml_escape(&value)
            )
        })
        .collect::<String>();
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n  <key>Label</key><string>{SERVICE_LABEL}</string>\n  <key>ProgramArguments</key><array><string>{executable}</string><string>run</string></array>\n  <key>EnvironmentVariables</key><dict>\n{environment}  </dict>\n  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key><true/>\n  <key>ThrottleInterval</key><integer>10</integer>\n  <key>StandardOutPath</key><string>{state}/usage-watch.log</string>\n  <key>StandardErrorPath</key><string>{state}/usage-watch.log</string>\n  <key>ProcessType</key><string>Background</string>\n</dict></plist>\n"
    ))
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn current_uid() -> String {
    std::env::var("UID")
        .ok()
        .filter(|value| value.parse::<u32>().is_ok())
        .unwrap_or_else(|| {
            // SAFETY: getuid takes no pointers and returns the calling
            // process's uid without touching Rust-managed memory.
            unsafe { libc::getuid().to_string() }
        })
}

#[cfg(test)]
mod tests;
