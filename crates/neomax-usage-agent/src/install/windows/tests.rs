use std::process::{ExitStatus, Output};
use std::sync::Mutex;
use std::time::Duration;

use crate::config::{AgentConfig, AgentPaths};
use crate::install::ServiceState;
use crate::install::runner::CommandRunner;
use crate::install::windows::{TASK_NAME, cmd_escape, install_with, status_with, uninstall_with};
use neomax_core::config::StatePaths;

#[derive(Default)]
struct FakeRunner {
    calls: Mutex<Vec<(String, Vec<String>, Duration)>>,
}

impl CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> anyhow::Result<Output> {
        self.calls.lock().unwrap().push((
            program.into(),
            args.iter().map(|arg| (*arg).into()).collect(),
            timeout,
        ));
        Ok(Output {
            status: ExitStatus::default(),
            stdout: if args.contains(&"/Query") {
                b"STATUS: RUNNING\n".to_vec()
            } else {
                Vec::new()
            },
            stderr: Vec::new(),
        })
    }
}

fn config(temp: &tempfile::TempDir) -> AgentConfig {
    AgentConfig::with_paths(AgentPaths::for_state(StatePaths::new(
        temp.path(),
        temp.path().join(".neomax"),
    )))
}

#[test]
fn install_writes_a_non_secret_task_and_starts_it() {
    let temp = tempfile::tempdir().unwrap();
    let config = config(&temp);
    let runner = FakeRunner::default();
    let report = install_with(&config, &runner).unwrap();
    assert_eq!(report.state, ServiceState::Active);
    let xml = std::fs::read_to_string(&config.paths.windows_task_xml).unwrap();
    let expected_shell = super::task_shell().unwrap();
    let (_, command_and_rest) = xml.split_once("<Command>").expect("task command element");
    let (command, rest) = command_and_rest
        .split_once("</Command>")
        .expect("closed task command element");
    assert!(
        !rest.contains("<Command>"),
        "duplicate task command element"
    );
    assert_eq!(command, super::xml_escape(&expected_shell).unwrap());
    #[cfg(windows)]
    {
        let shell = std::path::Path::new(&expected_shell);
        assert!(
            shell.is_absolute(),
            "unexpected task shell path: {expected_shell:?}"
        );
        assert!(
            shell
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("cmd.exe")),
            "unexpected task shell executable: {expected_shell:?}"
        );
        assert!(
            !expected_shell.chars().any(char::is_control) && !expected_shell.contains('"'),
            "unsafe task shell path: {expected_shell:?}"
        );
    }
    assert!(xml.contains("NEOMAX_HOME"));
    assert!(xml.contains("NEOMAX_USAGE_AGENT_BIN"));
    assert!(xml.contains("HOME"));
    assert!(xml.contains("USERPROFILE"));
    assert!(xml.contains("XDG_CONFIG_HOME"));
    assert!(xml.contains("APPDATA"));
    assert!(xml.contains("LOCALAPPDATA"));
    assert!(xml.contains("PATH"));
    assert!(xml.contains(TASK_NAME));
    assert!(!xml.contains("API_KEY"));
    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|(_, _, timeout)| *timeout >= Duration::from_secs(1))
    );
}

#[test]
fn status_uses_a_structured_query_and_scheduler_result_code() {
    let temp = tempfile::tempdir().unwrap();
    let config = config(&temp);
    let runner = FakeRunner::default();
    let report = status_with(&config, &runner).unwrap();
    assert_eq!(report.state, ServiceState::Active);
    let calls = runner.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].1.windows(2).any(|pair| pair == ["/FO", "CSV"]));
    assert!(calls[0].1.iter().any(|arg| *arg == "/V"));
}

#[test]
fn localized_status_labels_are_decoded_from_the_stable_running_result_code() {
    assert!(super::task_output_is_running(
        br#""\MACHINE","\Neomax\UsageAgent","N/A","En cours", "0x41301""#
    ));
    assert!(!super::task_output_is_running(
        "\"\\MACHINE\",\"\\Neomax\\UsageAgent\",\"N/A\",\"Prêt\", \"0x41300\"".as_bytes()
    ));
}

#[cfg(windows)]
#[test]
fn system_directory_decoder_rejects_untrusted_native_results() {
    let valid = "C:\\Windows\\System32";
    let mut buffer = [0u16; 64];
    let units = valid.encode_utf16().collect::<Vec<_>>();
    buffer[..units.len()].copy_from_slice(&units);
    assert_eq!(
        super::system_directory_from_utf16(&buffer, units.len() as u32).expect("valid path"),
        std::path::PathBuf::from(valid)
    );

    assert!(super::system_directory_from_utf16(&buffer, 0).is_err());
    assert!(super::system_directory_from_utf16(&buffer, buffer.len() as u32).is_err());

    let mut nul = buffer;
    nul[2] = 0;
    assert!(super::system_directory_from_utf16(&nul, units.len() as u32).is_err());

    let mut invalid_utf16 = buffer;
    invalid_utf16[0] = 0xD800;
    assert!(super::system_directory_from_utf16(&invalid_utf16, units.len() as u32).is_err());

    for value in [
        "Windows\\System32",
        "\\Windows\\System32",
        "C:Windows\\System32",
        "C:\\Windows\\..\\System32",
    ] {
        let units = value.encode_utf16().collect::<Vec<_>>();
        let mut candidate = [0u16; 64];
        candidate[..units.len()].copy_from_slice(&units);
        assert!(
            super::system_directory_from_utf16(&candidate, units.len() as u32).is_err(),
            "path should be rejected: {value}"
        );
    }
}

#[test]
fn cmd_values_round_trip_every_shell_metacharacter() {
    let value = r#"100%! & ^ | < > (parentheses) \"quoted\"/日本語"#;
    let encoded = cmd_escape(value).unwrap();
    assert!(encoded.contains("%%cd:~,%%"));
    assert!(encoded.contains("^!"));
    assert!(encoded.contains("^&"));
    assert!(encoded.contains("^^"));
    assert!(encoded.contains("^("));
    assert!(encoded.contains("^)"));
    assert!(encoded.contains("^\""));
    let decoded = encoded.replace("%%cd:~,%%", "%");
    assert_eq!(super::cmd_unescape(&decoded), value);
}

#[test]
fn cmd_values_reject_controls_and_newlines() {
    for value in ["line\nfeed", "carriage\rreturn", "nul\0byte"] {
        assert!(
            cmd_escape(value).is_err(),
            "value should be rejected: {value:?}"
        );
    }
}

#[test]
fn task_command_arguments_preserve_percent_path_literals() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = config(&temp);
    config.environment = config
        .environment
        .clone()
        .with_value("NEOMAX_HOME", r#"C:\Neomax\%PATH%\state"#);

    let arguments =
        super::command_arguments(&config, r#"C:\Neomax\%PATH%\usage-agent.exe"#).unwrap();
    assert!(arguments.starts_with("/d /e:on /v:off /s /c "));
    assert!(arguments.contains("%%cd:~,%%"));
}

#[cfg(unix)]
#[test]
fn install_refuses_a_preexisting_task_xml_symlink() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let config = config(&temp);
    std::fs::create_dir_all(config.paths.windows_task_xml.parent().unwrap()).unwrap();
    let target = temp.path().join("outside.xml");
    std::fs::write(&target, b"original").unwrap();
    symlink(&target, &config.paths.windows_task_xml).unwrap();
    let runner = FakeRunner::default();

    assert!(install_with(&config, &runner).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert!(
        std::fs::symlink_metadata(&config.paths.windows_task_xml)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[test]
fn task_xml_rejects_control_character_paths() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = config(&temp);
    config.executable = temp.path().join("neomax-\n").join("usage-agent");

    let error = super::task_xml(&config).unwrap_err();
    assert!(error.to_string().contains("control characters"));
}

#[test]
fn status_and_uninstall_are_injected_and_reversible() {
    let temp = tempfile::tempdir().unwrap();
    let config = config(&temp);
    let runner = FakeRunner::default();
    install_with(&config, &runner).unwrap();
    assert_eq!(
        status_with(&config, &runner).unwrap().state,
        ServiceState::Active
    );
    assert_eq!(
        uninstall_with(&config, &runner).unwrap().state,
        ServiceState::Inactive
    );
    assert!(!config.paths.windows_task_xml.exists());
}
