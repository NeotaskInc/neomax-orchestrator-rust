#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Output};

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let security = bin.join("security");
        std::fs::write(&security, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&security, std::fs::Permissions::from_mode(0o755)).unwrap();
        let provider = bin.join("codex");
        std::fs::write(&provider, r##"#!/bin/sh
if [ "$1" = "--version" ]; then printf 'codex-cli fixture\n'; exit 0; fi
printf '%s\n' "$*" >> "$(dirname "$0")/calls"
if [ "$1" = "login" ]; then
  if [ "$FIXTURE_LOGIN_FAIL" = "1" ]; then exit 7; fi
  printf '%s' '{"account_id":"fixture-account","email":"person@example.test","tokens":{"access_token":"fixture-token","refresh_token":"fixture-refresh"}}' > "$CODEX_HOME/auth.json"
  exit 0
fi
printf 'FIXTURE SESSION OPEN\n'
"##).unwrap();
        std::fs::set_permissions(&provider, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { root }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_neomax"));
        command
            .args(args)
            .current_dir(self.root.path())
            .env_clear()
            .env("HOME", self.root.path())
            .env("USERPROFILE", self.root.path())
            .env("NEOMAX_HOME", self.root.path().join("state"))
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.path().join("bin").display()),
            );
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn calls(&self) -> String {
        std::fs::read_to_string(self.root.path().join("bin/calls")).unwrap_or_default()
    }
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn one_provider_command_logs_in_then_launches_and_reuses_number_or_email() {
    let fixture = Fixture::new();
    success(&fixture.run(&["codex", "2"]));
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| *line == "login")
            .count(),
        1
    );
    assert!(fixture.calls().contains("service_tier=default"));
    success(&fixture.run(&["codex", "person@example.test"]));
    success(&fixture.run(&["codex", "2", "--codex-fast"]));
    assert_eq!(
        fixture
            .calls()
            .lines()
            .filter(|line| *line == "login")
            .count(),
        1
    );
    assert!(fixture.calls().contains("service_tier=fast"));
}

#[test]
fn failed_login_or_mismatched_email_never_starts_a_session() {
    let fixture = Fixture::new();
    let failed = fixture
        .command(&["codex", "2"])
        .env("FIXTURE_LOGIN_FAIL", "1")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert_eq!(fixture.calls().trim(), "login");
    let mismatch = fixture.run(&["codex", "different@example.test"]);
    assert!(!mismatch.status.success());
    assert!(
        String::from_utf8_lossy(&mismatch.stderr).contains("did not confirm the requested email")
    );
    assert!(fixture.calls().lines().all(|line| line == "login"));
}

#[test]
fn dry_run_and_invalid_launch_options_do_not_initiate_login() {
    let fixture = Fixture::new();
    let dry = fixture.run(&["codex", "person@example.test", "--dry-run", "--json"]);
    success(&dry);
    let report: serde_json::Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(report["login_required"], true);
    assert!(!fixture.run(&["codex", "2", "--detach"]).status.success());
    assert!(
        !fixture
            .run(&["codex", "2", "--engine", "kimi"])
            .status
            .success()
    );
    assert!(fixture.calls().is_empty());
    assert!(!fixture.root.path().join(".codex-acct2").exists());
}

#[test]
fn paused_authenticated_accounts_are_rejected_without_reauthenticating() {
    let fixture = Fixture::new();
    success(&fixture.run(&["codex", "2"]));
    let paths =
        neomax_core::StatePaths::new(fixture.root.path(), fixture.root.path().join("state"));
    neomax_core::accounts::AccountControlStore::new(&paths.cooldowns, &paths.paused)
        .set_paused(&fixture.root.path().join(".codex-acct2"), true)
        .unwrap();
    let calls = fixture.calls();
    assert!(!fixture.run(&["codex", "2"]).status.success());
    assert_eq!(fixture.calls(), calls);
}

#[test]
fn direct_orchestrator_command_preserves_the_existing_launch_plan() {
    let fixture = Fixture::new();
    let output = fixture.run(&["orchestrator", "--engine", "codex", "--dry-run", "--json"]);
    success(&output);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["orchestrator"], "codex");
    assert_eq!(report["worker_dispatch"], false);
    assert!(fixture.calls().is_empty());
}

#[test]
fn rotation_swaps_credentials_and_email_selection_follows_identity() {
    let fixture = Fixture::new();
    for (number, email) in [(2, "first@example.test"), (4, "second@example.test")] {
        let profile = fixture.root.path().join(format!(".codex-acct{number}"));
        std::fs::create_dir(&profile).unwrap();
        std::fs::write(profile.join("auth.json"), serde_json::json!({"account_id":email,"email":email,"tokens":{"access_token":"fixture-token","refresh_token":"fixture-refresh"}}).to_string()).unwrap();
        std::fs::write(profile.join("session-marker"), number.to_string()).unwrap();
    }
    let before = std::fs::read(fixture.root.path().join(".codex-acct2/auth.json")).unwrap();
    success(&fixture.run(&["codex", "2", "rotate", "--with", "4", "--dry-run", "--json"]));
    assert_eq!(
        std::fs::read(fixture.root.path().join(".codex-acct2/auth.json")).unwrap(),
        before
    );
    let output = fixture.run(&[
        "codex",
        "2",
        "rotate",
        "--with",
        "second@example.test",
        "--json",
    ]);
    success(&output);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["source_email_after"], "second@example.test");
    assert_eq!(report["replacement_email_after"], "first@example.test");
    assert_eq!(report["session_restarted"], false);
    for number in [2, 4] {
        assert_eq!(
            std::fs::read_to_string(
                fixture
                    .root
                    .path()
                    .join(format!(".codex-acct{number}/session-marker"))
            )
            .unwrap(),
            number.to_string()
        );
    }
    let selected = fixture.run(&["codex", "first@example.test", "--dry-run", "--json"]);
    success(&selected);
    let plan: serde_json::Value = serde_json::from_slice(&selected.stdout).unwrap();
    assert_eq!(plan["account"], "4");
    let legacy = fixture
        .command(&["run", "2", "rotate", "--with", "4", "--json"])
        .arg0("cdx")
        .output()
        .unwrap();
    success(&legacy);
    let report: serde_json::Value = serde_json::from_slice(&legacy.stdout).unwrap();
    assert_eq!(report["source_email_after"], "first@example.test");
    assert!(fixture.calls().is_empty());
}

#[test]
fn doctor_is_read_only_and_never_launches_a_provider() {
    let fixture = Fixture::new();
    let output = fixture.run(&["doctor", "--json"]);
    success(&output);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["read_only"], true);
    assert!(!fixture.root.path().join("state").exists());
    assert!(!fixture.root.path().join(".config").exists());
    assert!(fixture.calls().is_empty());
}

#[test]
fn doctor_rejects_invalid_effective_configuration_without_repairing_it() {
    let fixture = Fixture::new();
    let config = fixture.root.path().join(".config/neomax/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    let malformed = "reset_aware_ranking = 'invalid'\n";
    std::fs::write(&config, malformed).unwrap();
    let output = fixture.run(&["doctor", "--json"]);
    success(&output);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["settings_valid"], false);
    assert_eq!(std::fs::read_to_string(config).unwrap(), malformed);
    assert!(!fixture.root.path().join("state").exists());
    assert!(fixture.calls().is_empty());
}
