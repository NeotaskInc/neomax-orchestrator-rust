use std::collections::BTreeMap;
#[cfg(unix)]
use std::time::Duration;

use super::super::CommandRunner;
use super::super::CommandOutput;
use super::super::{DiscoveryCommand, LocalCommandRunner};
use crate::Engine;

#[test]
fn command_discovery_is_injected_and_scrubs_auth_environment() {
    let temp = tempfile::tempdir().unwrap();
    let environment = super::super::MapEnvironment::new([
        ("NEOMAX_OPENCODE_BIN".into(), "fixture-opencode".into()),
        ("OPENAI_API_KEY".into(), "secret-input".into()),
        ("OPENCODE_ZEN_API_KEY".into(), "secret-input".into()),
        ("OPENCODE_AUTH_CONTENT".into(), "secret-input".into()),
        ("PATH".into(), "/fixture/bin".into()),
    ])
    .with_home(temp.path())
    .with_current_dir(temp.path());
    let commands = super::fixtures::FixtureCommands::default().output(
        "fixture-opencode",
        CommandOutput {
            success: true,
            stdout: br#"[{"id":"opencode/big-pickle"},{"id":"local/big-pickle"}]"#.to_vec(),
            timed_out: false,
            truncated: false,
        },
    );
    let models = super::super::commands::model_ids(Engine::Opencode, &environment, &commands);
    assert_eq!(models, ["local/big-pickle", "opencode/big-pickle"]);
    let seen = commands.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].args, ["models"]);
    assert!(!seen[0].safe_environment.contains_key("OPENAI_API_KEY"));
    assert!(!seen[0]
        .safe_environment
        .contains_key("OPENCODE_ZEN_API_KEY"));
    assert!(!seen[0]
        .safe_environment
        .contains_key("OPENCODE_AUTH_CONTENT"));
}

#[test]
fn kimi_model_discovery_uses_provider_json_and_preserves_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let environment = super::super::MapEnvironment::new([
        ("NEOMAX_KIMI_BIN".into(), "fixture-kimi".into()),
        ("KIMI_CODE_HOME".into(), "/fixture/kimi-profile".into()),
        ("PATH".into(), "/fixture/bin".into()),
    ])
    .with_home(temp.path())
    .with_current_dir(temp.path());
    let commands = super::fixtures::FixtureCommands::default().output(
        "fixture-kimi",
        CommandOutput {
            success: true,
            stdout: br#"{
                "providers": {
                    "kimi-code": {"models": ["k3", "k2.7"]}
                },
                "models": {
                    "k3": {"provider": "kimi-code"},
                    "local-alias": {"provider": "local"}
                }
            }"#
            .to_vec(),
            timed_out: false,
            truncated: false,
        },
    );

    let models = super::super::commands::model_ids(Engine::Kimi, &environment, &commands);
    assert_eq!(models, ["k2.7", "k3", "local-alias"]);
    let seen = commands.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].args, ["provider", "list", "--json"]);
    assert_eq!(
        seen[0].safe_environment.get("KIMI_CODE_HOME"),
        Some(&"/fixture/kimi-profile".into())
    );
}

#[test]
fn kimi_model_discovery_fails_closed_for_non_json_output() {
    let temp = tempfile::tempdir().unwrap();
    let environment = super::super::MapEnvironment::new([("PATH".into(), "/fixture/bin".into())])
        .with_home(temp.path())
        .with_current_dir(temp.path());
    let commands = super::fixtures::FixtureCommands::default().output(
        "kimi",
        CommandOutput {
            success: true,
            stdout: b"k3\nk2.7\n".to_vec(),
            timed_out: false,
            truncated: false,
        },
    );
    assert!(super::super::commands::model_ids(Engine::Kimi, &environment, &commands).is_empty());
}

#[cfg(unix)]
fn shell_command(script: &str) -> DiscoveryCommand {
    DiscoveryCommand {
        program: "sh".into(),
        args: vec!["-c".into(), script.into()],
        cwd: None,
        safe_environment: BTreeMap::new(),
    }
}

#[test]
fn local_runner_uses_the_process_directory_when_cwd_is_omitted() {
    let command = DiscoveryCommand {
        program: std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        args: vec![
            "--exact".into(),
            "providers::catalog::tests::commands::local_runner_child_fixture".into(),
        ],
        cwd: None,
        safe_environment: BTreeMap::new(),
    };
    let output = LocalCommandRunner::default().run(&command).unwrap();
    assert!(output.success);
}

#[test]
fn local_runner_child_fixture() {}

#[cfg(unix)]
#[test]
fn local_runner_terminates_a_hanging_discovery_process() {
    let runner = LocalCommandRunner::new(Duration::from_millis(50), 1024);
    let output = runner.run(&shell_command("while :; do :; done")).unwrap();
    assert!(!output.success);
    assert!(output.timed_out);
    assert!(!output.truncated);
}

#[cfg(unix)]
#[test]
fn local_runner_caps_oversized_discovery_output() {
    let runner = LocalCommandRunner::new(Duration::from_secs(2), 4096);
    let output = runner
        .run(&shell_command("head -c 100000 /dev/zero"))
        .unwrap();
    assert!(!output.success);
    assert!(!output.timed_out);
    assert!(output.truncated);
    assert!(output.stdout.len() <= 4096);
}

#[cfg(unix)]
#[test]
fn local_runner_caps_oversized_stderr_as_well_as_stdout() {
    let runner = LocalCommandRunner::new(Duration::from_secs(2), 4096);
    let output = runner
        .run(&shell_command("head -c 100000 /dev/zero >&2"))
        .unwrap();
    assert!(!output.success);
    assert!(!output.timed_out);
    assert!(output.truncated);
}
