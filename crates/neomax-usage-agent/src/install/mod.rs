#[cfg(target_os = "macos")]
mod launchd;
#[cfg(any(target_os = "linux", all(test, unix)))]
mod linux;
mod runner;
#[cfg(any(target_os = "windows", test))]
mod windows;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

use anyhow::{Result, bail};
use neomax_core::atomic::write_bytes_atomic;
use neomax_core::io::{is_rooted_but_not_absolute, os_str_to_utf8, path_to_string, path_to_utf8};
use serde::Serialize;

use crate::config::{AgentConfig, AgentPaths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ServiceState {
    #[cfg(any(target_os = "macos", target_os = "windows", test))]
    Loaded,
    #[cfg(any(target_os = "linux", target_os = "windows", test))]
    Active,
    Inactive,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceReport {
    pub platform: String,
    pub path: String,
    pub state: ServiceState,
    pub detail: String,
}

pub(crate) fn install(config: &AgentConfig) -> Result<ServiceReport> {
    #[cfg(target_os = "macos")]
    {
        return launchd::install(config);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::install(config);
    }
    #[cfg(target_os = "windows")]
    {
        return windows::install(config);
    }
    #[allow(unreachable_code)]
    unsupported(
        &config.paths,
        "service installation is not implemented on this platform",
    )
}

pub(crate) fn uninstall(config: &AgentConfig) -> Result<ServiceReport> {
    #[cfg(target_os = "macos")]
    {
        return launchd::uninstall(config);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::uninstall(config);
    }
    #[cfg(target_os = "windows")]
    {
        return windows::uninstall(config);
    }
    #[allow(unreachable_code)]
    unsupported(
        &config.paths,
        "service uninstallation is not implemented on this platform",
    )
}

pub(crate) fn status(config: &AgentConfig) -> Result<ServiceReport> {
    #[cfg(target_os = "macos")]
    {
        return launchd::status(config);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::status(config);
    }
    #[cfg(target_os = "windows")]
    {
        return windows::status(config);
    }
    #[allow(unreachable_code)]
    unsupported(
        &config.paths,
        "service status is not implemented on this platform",
    )
}

pub(crate) fn ensure(config: &AgentConfig) -> Result<ServiceReport> {
    let report = status(config)?;
    if !ensure_requires_install(&report) {
        return Ok(report);
    }
    install(config)
}

fn ensure_requires_install(report: &ServiceReport) -> bool {
    match report.state {
        #[cfg(any(target_os = "linux", target_os = "windows", test))]
        ServiceState::Active => false,
        #[cfg(any(target_os = "macos", target_os = "windows", test))]
        ServiceState::Loaded if report.platform == "macos" => false,
        ServiceState::Inactive | ServiceState::Unknown | ServiceState::Unsupported => true,
        #[cfg(any(target_os = "macos", target_os = "windows", test))]
        ServiceState::Loaded => true,
    }
}

fn unsupported(paths: &AgentPaths, detail: &str) -> Result<ServiceReport> {
    Ok(ServiceReport {
        platform: std::env::consts::OS.into(),
        path: path_to_string("usage-agent service path", &paths.launchd_plist)?,
        state: ServiceState::Unsupported,
        detail: detail.into(),
    })
}

pub(crate) fn environment_values(config: &AgentConfig) -> Result<BTreeMap<String, String>> {
    let mut values = config.environment.values().clone();
    values.insert(
        "NEOMAX_HOME".into(),
        path_to_string("NEOMAX_HOME", &config.paths.state.state)?,
    );
    values.insert(
        "NEOMAX_USAGE_AGENT_BIN".into(),
        path_to_string("NEOMAX_USAGE_AGENT_BIN", &config.executable)?,
    );
    values.insert(
        "NEOMAX_CLI_BIN".into(),
        path_to_string("NEOMAX_CLI_BIN", &config.neomax_cli)?,
    );
    values.insert(
        "NEOMAX_USAGE_POLL".into(),
        config.poll_interval.as_secs().to_string(),
    );
    values.insert(
        "NEOMAX_USAGE_RECENT_DAYS".into(),
        config.recent_days.to_string(),
    );
    values.insert(
        "NEOMAX_ROTATE_TICK".into(),
        config.rotation_interval.as_secs().to_string(),
    );
    values.insert(
        "NEOMAX_KEEPALIVE_EVERY".into(),
        config.keepalive_interval.as_secs().to_string(),
    );
    values.insert(
        "NEOMAX_WORKTREE_TIDY_EVERY".into(),
        config
            .worktree_tidy_interval
            .map_or(0, |interval| interval.as_secs())
            .to_string(),
    );
    values.insert(
        "NEOMAX_WORKTREE_TIDY_TIMEOUT_SECS".into(),
        config.worktree_tidy_timeout.as_secs().to_string(),
    );
    values.insert(
        "NEOMAX_MAINTENANCE_TIMEOUT_SECS".into(),
        config.maintenance_timeout.as_secs().to_string(),
    );
    for (key, value) in &values {
        if key.is_empty() || key.chars().any(char::is_control) {
            bail!("service environment key is invalid: {key:?}");
        }
        if value.chars().any(char::is_control) {
            bail!("service environment value for {key} contains a control character");
        }
    }
    Ok(values)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn unix_environment_values(config: &AgentConfig) -> Result<BTreeMap<String, String>> {
    let mut values = environment_values(config)?;
    values.retain(|key, _| !matches!(key.as_str(), "USERPROFILE" | "APPDATA" | "LOCALAPPDATA"));
    Ok(values)
}

pub(crate) fn validate_service_paths(config: &AgentConfig) -> Result<()> {
    for (label, path) in [
        ("usage-agent home path", config.paths.home.as_path()),
        ("usage-agent state path", config.paths.state.state.as_path()),
        ("launchd service path", config.paths.launchd_plist.as_path()),
        ("systemd service path", config.paths.systemd_unit.as_path()),
        (
            "Windows service path",
            config.paths.windows_task_xml.as_path(),
        ),
        ("usage-agent executable path", config.executable.as_path()),
        ("Neomax CLI path", config.neomax_cli.as_path()),
    ] {
        let value = path_to_utf8(label, path)?;
        if !path.is_absolute() || is_rooted_but_not_absolute(path) {
            bail!("{label} must be an absolute path");
        }
        if value.chars().any(char::is_control) {
            bail!("{label} must not contain control characters");
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        let value = os_str_to_utf8("PATH", &path)?;
        if value.chars().any(char::is_control) {
            bail!("PATH must not contain control characters");
        }
    }
    Ok(())
}

pub(crate) fn write_service_artifact(path: &Path, contents: &str) -> Result<()> {
    let path_text = path_to_utf8("service artifact path", path)?;
    if path_text.chars().any(char::is_control) {
        bail!("service artifact path must not contain control characters");
    }
    let _parent_guard = ensure_parent_guard(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link_like(&metadata) => {
            bail!("refusing to replace symlink or reparse service artifact {path_text}");
        }
        Ok(metadata) if !metadata.file_type().is_file() => {
            bail!("service artifact is not a regular file: {path_text}");
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    write_bytes_atomic(path, contents.as_bytes())?;
    let metadata = fs::symlink_metadata(path)?;
    if is_link_like(&metadata) || !metadata.file_type().is_file() {
        bail!("service artifact is not a private regular file: {path_text}");
    }
    neomax_core::io::verify_private_path(path)?;
    Ok(())
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn windows_environment_values(config: &AgentConfig) -> Result<BTreeMap<String, String>> {
    environment_values(config)
}

fn ensure_parent(path: &Path) -> Result<()> {
    let _guard = ensure_parent_guard(path)?;
    Ok(())
}

fn ensure_parent_guard(path: &Path) -> Result<Vec<neomax_core::io::PathGuard>> {
    let parent = path
        .parent()
        .map(|parent| {
            if parent.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                parent.to_path_buf()
            }
        })
        .ok_or_else(|| {
            anyhow::anyhow!("service artifact path has no parent: {}", path.display())
        })?;
    validate_path_shape(path)?;

    // Walk upwards without following links until an existing directory is
    // found. Missing components are then created one at a time so every
    // component is checked before the next one is traversed.
    let mut missing = Vec::new();
    let mut current = parent;
    let base = loop {
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                validate_directory_component(&current, &metadata)?;
                break current;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current.clone());
                let next = current.parent().ok_or_else(|| {
                    anyhow::anyhow!(
                        "could not find an existing ancestor for service artifact path: {}",
                        path.display()
                    )
                })?;
                if next == current {
                    bail!(
                        "could not find an existing ancestor for service artifact path: {}",
                        path.display()
                    );
                }
                current = if next.as_os_str().is_empty() {
                    Path::new(".").to_path_buf()
                } else {
                    next.to_path_buf()
                };
            }
            Err(error) => return Err(error.into()),
        }
    };

    let mut guards = vec![neomax_core::io::PathGuard::for_directory(&base)?];
    for candidate in missing.into_iter().rev() {
        match fs::symlink_metadata(&candidate) {
            Ok(metadata) => validate_directory_component(&candidate, &metadata)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&candidate)?;
            }
            Err(error) => return Err(error.into()),
        }
        let metadata = fs::symlink_metadata(&candidate)?;
        validate_directory_component(&candidate, &metadata)?;
        guards.push(neomax_core::io::PathGuard::for_directory(&candidate)?);
    }
    Ok(guards)
}

fn validate_path_shape(path: &Path) -> Result<()> {
    if path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        bail!(
            "service artifact paths must not contain '.' or '..' components: {}",
            path.display()
        );
    }
    Ok(())
}

fn validate_directory_component(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    if is_link_like(metadata) {
        bail!(
            "refusing a symlink or reparse service artifact ancestor: {}",
            path.display()
        );
    }
    if !metadata.file_type().is_dir() {
        bail!(
            "service artifact ancestor is not a directory: {}",
            path.display()
        );
    }
    Ok(())
}

fn is_link_like(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ServiceReport, ServiceState, ensure_parent, ensure_requires_install, environment_values,
        write_service_artifact,
    };
    use crate::config::{AgentConfig, AgentPaths, ServiceEnvironment};
    use neomax_core::config::StatePaths;

    #[test]
    fn ensure_reuses_loaded_or_active_services() {
        assert!(!ensure_requires_install(&ServiceReport {
            platform: "macos".into(),
            path: "/fixture/service".into(),
            state: ServiceState::Loaded,
            detail: "loaded".into(),
        }));
        assert!(!ensure_requires_install(&ServiceReport {
            platform: "linux".into(),
            path: "/fixture/service".into(),
            state: ServiceState::Active,
            detail: "active".into(),
        }));
    }

    #[test]
    fn ensure_installs_missing_or_unknown_services() {
        for state in [
            ServiceState::Inactive,
            ServiceState::Unknown,
            ServiceState::Unsupported,
            ServiceState::Loaded,
        ] {
            assert!(ensure_requires_install(&ServiceReport {
                platform: "windows".into(),
                path: "/fixture/service".into(),
                state,
                detail: "not running".into(),
            }));
        }
    }

    #[test]
    fn service_environment_rejects_control_values() {
        let temp = tempfile::tempdir().unwrap();
        let paths =
            AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
        let mut config = AgentConfig::with_paths(paths);
        config.environment = ServiceEnvironment::default().with_value("HOME", "bad\nvalue");
        assert!(environment_values(&config).is_err());
    }

    #[test]
    fn service_environment_propagates_worktree_tidy_interval_and_opt_out() {
        let temp = tempfile::tempdir().unwrap();
        let paths =
            AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
        let mut config = AgentConfig::with_paths(paths);
        assert_eq!(
            environment_values(&config).unwrap()["NEOMAX_WORKTREE_TIDY_EVERY"],
            "600"
        );
        assert_eq!(
            environment_values(&config).unwrap()["NEOMAX_WORKTREE_TIDY_TIMEOUT_SECS"],
            "300"
        );
        config.worktree_tidy_interval = None;
        assert_eq!(
            environment_values(&config).unwrap()["NEOMAX_WORKTREE_TIDY_EVERY"],
            "0"
        );
    }

    #[cfg(unix)]
    #[test]
    fn service_artifact_rejects_preexisting_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("outside");
        let artifact = temp.path().join("service.xml");
        std::fs::write(&target, b"original").unwrap();
        symlink(&target, &artifact).unwrap();

        assert!(write_service_artifact(&artifact, "replacement").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert!(
            std::fs::symlink_metadata(&artifact)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[cfg(unix)]
    #[test]
    fn service_artifact_rejects_a_symlink_parent_before_creating_output() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let real_parent = temp.path().join("real-parent");
        let linked_parent = temp.path().join("linked-parent");
        std::fs::create_dir(&real_parent).unwrap();
        symlink(&real_parent, &linked_parent).unwrap();
        let artifact = linked_parent.join("service.xml");

        assert!(write_service_artifact(&artifact, "replacement").is_err());
        assert!(!real_parent.join("service.xml").exists());
    }

    #[cfg(unix)]
    #[test]
    fn ensure_parent_rejects_a_symlink_ancestor() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let real_parent = temp.path().join("real-parent");
        let linked_parent = temp.path().join("linked-parent");
        std::fs::create_dir(&real_parent).unwrap();
        symlink(&real_parent, &linked_parent).unwrap();

        assert!(ensure_parent(&linked_parent.join("nested/service.xml")).is_err());
        assert!(!real_parent.join("nested").exists());
    }

    #[cfg(windows)]
    #[test]
    fn service_artifact_rejects_a_windows_reparse_parent() {
        let temp = tempfile::tempdir().unwrap();
        let real_parent = temp.path().join("real-parent");
        let linked_parent = temp.path().join("linked-parent");
        std::fs::create_dir(&real_parent).unwrap();
        if std::os::windows::fs::symlink_dir(&real_parent, &linked_parent).is_err() {
            eprintln!("skipping reparse test: directory-link creation is unavailable");
            return;
        }
        let artifact = linked_parent.join("service.xml");

        assert!(write_service_artifact(&artifact, "replacement").is_err());
        assert!(!real_parent.join("service.xml").exists());
    }

    #[cfg(windows)]
    #[test]
    fn ensure_parent_rejects_a_windows_reparse_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let real_parent = temp.path().join("real-parent");
        let linked_parent = temp.path().join("linked-parent");
        std::fs::create_dir(&real_parent).unwrap();
        if std::os::windows::fs::symlink_dir(&real_parent, &linked_parent).is_err() {
            eprintln!("skipping reparse test: directory-link creation is unavailable");
            return;
        }

        assert!(ensure_parent(&linked_parent.join("nested/service.xml")).is_err());
        assert!(!real_parent.join("nested").exists());
    }

    #[test]
    fn service_artifact_is_private_regular_file_after_atomic_write() {
        let temp = tempfile::tempdir().unwrap();
        let artifact = temp.path().join("service.xml");
        write_service_artifact(&artifact, "service").unwrap();
        let metadata = std::fs::symlink_metadata(&artifact).unwrap();
        assert!(metadata.file_type().is_file());
        assert_eq!(std::fs::read(&artifact).unwrap(), b"service");
    }
}
