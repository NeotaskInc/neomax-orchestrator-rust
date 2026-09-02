use std::path::Path;

use neomax_core::Engine;

use super::super::actions;
use super::super::profiles::{DetectedAuth, ManagedProfile};
use super::support::{profile, provider_profile};

#[test]
fn codex_status_keeps_the_reference_three_account_baseline_without_creating_dirs() {
    let home = Path::new("/fixture/home");
    let profiles = actions::status_profiles(
        Engine::Codex,
        vec![profile(Engine::Codex, "2", Some(DetectedAuth::OAuth))],
        home,
        home,
    )
    .unwrap();
    assert_eq!(
        profiles
            .iter()
            .map(ManagedProfile::account)
            .collect::<Vec<_>>(),
        vec!["1", "2", "3"]
    );
    assert_eq!(profiles[0].profile.path, home.join(".codex"));
    assert_eq!(profiles[2].profile.path, home.join(".codex-acct3"));
    assert!(profiles[0].auth.is_none());
    assert!(profiles[2].auth.is_none());
}

#[test]
fn codex_status_warns_when_profiles_share_one_local_account_identity() {
    let temp = tempfile::tempdir().unwrap();
    let mut profiles = Vec::new();
    for account in ["1", "2"] {
        let path = temp.path().join(format!("codex-{account}"));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("auth.json"),
            format!(
                r#"{{"tokens":{{"access_token":"fixture-{account}","account_id":"shared-account"}}}}"#
            ),
        )
        .unwrap();
        profiles.push(ManagedProfile {
            profile: provider_profile(Engine::Codex, account, path),
            auth: Some(DetectedAuth::OAuth),
        });
    }

    let warnings = actions::duplicate_codex_warnings(Engine::Codex, &profiles);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("accounts 1, 2"));
    assert!(warnings[0].contains("refresh-token families"));
    assert!(!warnings[0].contains("shared-account"));
}

#[test]
fn codex_whoami_uses_sanitized_local_identity_and_discards_provider_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("codex");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("auth.json"),
        r#"{"tokens":{"access_token":"raw-access-token","account_id":"account-123"}}"#,
    )
    .unwrap();
    let profile = ManagedProfile {
        profile: provider_profile(Engine::Codex, "1", path),
        auth: Some(DetectedAuth::OAuth),
    };
    let output = actions::codex_whoami_output(
        &profile,
        &super::super::process::ProcessOutcome {
            status_code: Some(0),
            success: true,
            stdout: b"email=person@example.test token=raw-provider-token\n".to_vec(),
            stderr: Vec::new(),
        },
    );
    assert!(output.contains("authenticated via oauth"));
    assert!(output.contains("account identity acct-"));
    assert!(!output.contains("account-123"));
    assert!(!output.contains("person@example.test"));
    assert!(!output.contains("raw-provider-token"));
}
