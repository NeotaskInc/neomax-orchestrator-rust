use crate::providers::catalog::{CommandOutput, ProviderDiscovery};

use super::super::fixtures;
#[test]
#[cfg(target_os = "macos")]
fn provider_discovery_merges_claude_keychain_auth_without_returning_secret_output() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let fs = fixtures::FixtureFs::default().dir(home.join(".claude"));
    let environment = fixtures::environment(home);
    let commands = fixtures::FixtureCommands::default().output(
        "security",
        CommandOutput {
            success: true,
            stdout: b"keychain metadata secret-output".to_vec(),
            timed_out: false,
            truncated: false,
        },
    );
    let discovery = ProviderDiscovery {
        environment: &environment,
        filesystem: &fs,
        commands: &commands,
    };
    let snapshot = discovery.discover(crate::Engine::Claude).unwrap();
    let profile = snapshot
        .profiles
        .iter()
        .find(|profile| profile.path == home.join(".claude"))
        .unwrap();
    assert!(profile.eligibility.authenticated);
    assert!(profile.eligibility.managed_pool_eligible);
    assert!(!format!("{snapshot:?}").contains("secret-output"));
    let seen = commands.seen.lock().unwrap();
    let security = seen
        .iter()
        .find(|command| command.program == "security")
        .unwrap();
    assert!(!security.safe_environment.contains_key("OPENAI_API_KEY"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn provider_discovery_does_not_invoke_macos_keychain_on_other_hosts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let fs = fixtures::FixtureFs::default().dir(home.join(".claude"));
    let environment = fixtures::environment(home);
    let commands = fixtures::FixtureCommands::default().output(
        "security",
        CommandOutput {
            success: true,
            stdout: b"keychain metadata secret-output".to_vec(),
            timed_out: false,
            truncated: false,
        },
    );
    let discovery = ProviderDiscovery {
        environment: &environment,
        filesystem: &fs,
        commands: &commands,
    };
    let snapshot = discovery.discover(crate::Engine::Claude).unwrap();
    let profile = snapshot
        .profiles
        .iter()
        .find(|profile| profile.path == home.join(".claude"))
        .unwrap();
    assert!(!profile.eligibility.authenticated);
    assert!(commands
        .seen
        .lock()
        .unwrap()
        .iter()
        .all(|command| command.program != "security"));
}

#[cfg(unix)]
#[test]
fn keychain_service_rejects_non_utf8_profile_paths() {
    use std::os::unix::ffi::OsStringExt;

    let profile = std::path::PathBuf::from(std::ffi::OsString::from_vec(
        b"/tmp/neomax-\xff/.claude".to_vec(),
    ));
    let home = std::path::Path::new("/tmp/neomax-home");
    let error =
        crate::providers::catalog::checked_claude_keychain_service(&profile, home).unwrap_err();
    assert!(error.to_string().contains("Claude profile path"));
    assert!(error.to_string().contains("UTF-8"));
}
