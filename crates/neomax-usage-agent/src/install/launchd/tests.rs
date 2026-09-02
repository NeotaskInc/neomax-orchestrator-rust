use std::process::{ExitStatus, Output};
use std::sync::Mutex;
use std::time::Duration;
#[cfg(unix)]
use std::{os::unix::ffi::OsStringExt, path::PathBuf};

use crate::config::{AgentConfig, AgentPaths};
use crate::config::{LEGACY_SERVICE_LABEL, SERVICE_LABEL};
use crate::install::ServiceState;
use crate::install::launchd::{install_with, status_with, uninstall_with};
use crate::install::runner::CommandRunner;
use neomax_core::config::StatePaths;

#[derive(Default)]
struct FakeRunner {
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str], _timeout: Duration) -> anyhow::Result<Output> {
        self.calls.lock().unwrap().push((
            program.into(),
            args.iter().map(|arg| (*arg).into()).collect(),
        ));
        Ok(Output {
            status: ExitStatus::default(),
            stdout: if args == ["list"] {
                b"-\t0\tio.neomax.usagewatch\n".to_vec()
            } else {
                Vec::new()
            },
            stderr: Vec::new(),
        })
    }
}

#[test]
fn install_writes_a_non_secret_launchd_program() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
    let mut config = AgentConfig::with_paths(paths.clone());
    config.executable = std::path::PathBuf::from("/tmp/bin/neomax & usage");
    let runner = FakeRunner::default();
    let report = install_with(&config, &runner).unwrap();
    assert_eq!(report.state, ServiceState::Loaded);
    let plist = std::fs::read_to_string(paths.launchd_plist).unwrap();
    assert!(plist.contains("<string>run</string>"));
    assert!(plist.contains("neomax &amp; usage"));
    assert!(plist.contains("NEOMAX_USAGE_POLL"));
    assert!(plist.contains("<key>HOME</key>"));
    assert!(plist.contains("<key>PATH</key>") || !plist.contains("PATH"));
    assert!(!plist.contains("<key>APPDATA</key>"));
    assert!(!plist.contains("<key>USERPROFILE</key>"));
    assert!(!plist.contains("API_KEY"));
    assert!(
        runner
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|(_, args)| !args.iter().any(|arg| arg == LEGACY_SERVICE_LABEL))
    );
}

#[test]
fn uninstall_is_the_only_legacy_launchd_label_migration_path() {
    assert_eq!(LEGACY_SERVICE_LABEL, "io.cmax.usagewatch");
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
    let config = AgentConfig::with_paths(paths.clone());
    std::fs::create_dir_all(paths.launchd_plist.parent().unwrap()).unwrap();
    std::fs::write(&paths.launchd_plist, "current").unwrap();
    let legacy_plist = paths
        .launchd_plist
        .with_file_name(format!("{LEGACY_SERVICE_LABEL}.plist"));
    std::fs::write(&legacy_plist, "legacy").unwrap();
    let runner = FakeRunner::default();

    let report = uninstall_with(&config, &runner).unwrap();

    assert_eq!(report.state, ServiceState::Inactive);
    assert!(!paths.launchd_plist.exists());
    assert!(!legacy_plist.exists());
    let calls = runner.calls.lock().unwrap();
    let domain = format!("gui/{}", super::current_uid());
    let current_bootout = vec!["bootout".into(), domain.clone(), SERVICE_LABEL.into()];
    let legacy_bootout = vec!["bootout".into(), domain, LEGACY_SERVICE_LABEL.into()];
    assert!(calls.iter().any(|(_, args)| { args == &current_bootout }));
    assert!(calls.iter().any(|(_, args)| args == &legacy_bootout));
}

#[test]
fn status_is_injected_and_does_not_spawn_a_real_launchctl() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
    let config = AgentConfig::with_paths(paths);
    let runner = FakeRunner::default();
    let report = status_with(&config, &runner).unwrap();
    assert_eq!(report.state, ServiceState::Loaded);
    assert_eq!(runner.calls.lock().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn launchd_plist_rejects_non_utf8_executable_paths() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
    let mut config = AgentConfig::with_paths(paths);
    config.executable = PathBuf::from(std::ffi::OsString::from_vec(
        b"/tmp/neomax-\xff/usage-agent".to_vec(),
    ));

    let error = super::plist(&config).unwrap_err();
    assert!(error.to_string().contains("usage-agent executable path"));
    assert!(error.to_string().contains("UTF-8"));
}

#[cfg(unix)]
#[test]
fn install_refuses_a_preexisting_launchd_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let paths = AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
    std::fs::create_dir_all(paths.launchd_plist.parent().unwrap()).unwrap();
    let target = temp.path().join("outside.plist");
    std::fs::write(&target, b"original").unwrap();
    symlink(&target, &paths.launchd_plist).unwrap();
    let config = AgentConfig::with_paths(paths.clone());
    let runner = FakeRunner::default();

    assert!(install_with(&config, &runner).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert!(
        std::fs::symlink_metadata(&paths.launchd_plist)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[test]
fn launchd_plist_rejects_control_character_paths() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
    let mut config = AgentConfig::with_paths(paths);
    config.executable = "/tmp/neomax-\n/usage-agent".into();

    let error = super::plist(&config).unwrap_err();
    assert!(error.to_string().contains("control characters"));
}
