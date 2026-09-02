use std::fs;

use neomax_core::Engine;
use neomax_core::providers::catalog::{self, RealFileSystem};

use super::super::actions;
use super::super::profiles::ManagedProfile;
use super::support::provider_profile;

#[test]
fn grok_whoami_renders_local_identity_metadata_without_secrets_or_paths() {
    let temp = tempfile::tempdir().unwrap();
    let profile_path = temp.path().join("grok-profile");
    fs::create_dir_all(&profile_path).unwrap();
    fs::write(
        profile_path.join("auth.json"),
        br#"{"xai::oidc":{"auth_mode":"oidc","key":"raw-api-token","email":"person@example.test","first_name":"Ada","last_name":"Lovelace","team_name":"Analytical Engine"}}"#,
    )
    .unwrap();
    let identity =
        catalog::grok_auth_identity(&profile_path, &RealFileSystem).expect("Grok identity");
    let profile = ManagedProfile {
        profile: provider_profile(Engine::Grok, "1", profile_path.clone()),
        auth: None,
    };

    let output = actions::grok_whoami_output(&profile, Some(&identity));

    assert!(output.contains("method: OAuth"));
    assert!(output.contains("email: person@example.test"));
    assert!(output.contains("name: Ada Lovelace"));
    assert!(output.contains("team: Analytical Engine"));
    assert!(!output.contains("raw-api-token"));
    assert!(!output.contains(&profile_path.display().to_string()));
}

#[test]
fn grok_whoami_keeps_a_generic_authenticated_fallback() {
    let profile = ManagedProfile {
        profile: provider_profile(Engine::Grok, "1", "/fixture/grok".into()),
        auth: Some(super::super::profiles::DetectedAuth::ApiKey),
    };

    assert_eq!(
        actions::grok_whoami_output(&profile, None),
        "authenticated via api-key\n"
    );
}
