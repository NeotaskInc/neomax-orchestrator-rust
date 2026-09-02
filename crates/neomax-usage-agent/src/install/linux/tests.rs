use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::{ExitStatus, Output};
use std::sync::Mutex;
use std::time::Duration;

use crate::config::{AgentConfig, AgentPaths};
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
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }
}

#[test]
fn systemd_unit_rejects_non_utf8_executable_paths() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
    let mut config = AgentConfig::with_paths(paths);
    config.executable = PathBuf::from(std::ffi::OsString::from_vec(
        b"/tmp/neomax-\xff/usage-agent".to_vec(),
    ));

    let error = super::unit(&config).unwrap_err();
    assert!(error.to_string().contains("usage-agent executable path"));
    assert!(error.to_string().contains("UTF-8"));
}

#[cfg(unix)]
#[test]
fn install_refuses_a_preexisting_systemd_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let paths = AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
    std::fs::create_dir_all(paths.systemd_unit.parent().unwrap()).unwrap();
    let target = temp.path().join("outside.service");
    std::fs::write(&target, b"original").unwrap();
    symlink(&target, &paths.systemd_unit).unwrap();
    let config = AgentConfig::with_paths(paths.clone());
    let runner = FakeRunner::default();

    assert!(super::install_with(&config, &runner).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert!(
        std::fs::symlink_metadata(&paths.systemd_unit)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[test]
fn systemd_unit_rejects_control_character_paths() {
    let temp = tempfile::tempdir().unwrap();
    let paths = AgentPaths::for_state(StatePaths::new(temp.path(), temp.path().join(".neomax")));
    let mut config = AgentConfig::with_paths(paths);
    config.executable = "/tmp/neomax-\n/usage-agent".into();

    let error = super::unit(&config).unwrap_err();
    assert!(error.to_string().contains("control characters"));
}

#[test]
fn user_unit_uses_the_config_home_and_runtime_environment() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("xdg-config");
    let appdata = temp.path().join("appdata");
    let paths = AgentPaths::for_state_with_roots(
        StatePaths::new(temp.path(), temp.path().join("state")),
        config_home.clone(),
        appdata,
    );
    let config = AgentConfig::with_paths(paths.clone());
    let rendered = super::unit(&config).unwrap();
    assert_eq!(
        paths.systemd_unit,
        config_home.join("systemd/user/neomax-usage-agent.service")
    );
    assert!(rendered.contains("WorkingDirectory=\""));
    assert!(rendered.contains("Environment=\"HOME="));
    assert!(rendered.contains("Environment=\"XDG_CONFIG_HOME="));
    assert!(!rendered.contains("APPDATA="));
    assert!(!rendered.contains("USERPROFILE="));
}

#[test]
fn systemd_escapes_specifiers_and_quotes_without_shell_expansion() {
    let escaped = super::systemd_escape("100% \"quoted\" $value");
    assert_eq!(escaped, r#"100%% \"quoted\" $value"#);
    assert_eq!(
        super::systemd_word("/tmp/a path/100%/quote\".bin"),
        r#""/tmp/a path/100%%/quote\".bin""#
    );
}
