use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use super::*;
use crate::providers::catalog::{
    AuthMethod, AuthStatus, MapEnvironment, ProfileEligibility, RealFileSystem,
};

fn snapshot(engine: Engine, account: &str, path: PathBuf) -> ProfileSnapshot {
    ProfileSnapshot {
        engine,
        account: account.into(),
        path,
        reserved: false,
        auth: AuthStatus::Authenticated {
            methods: vec![AuthMethod::OAuth],
        },
        eligibility: ProfileEligibility {
            credential_present: true,
            authenticated: true,
            worker_eligible: true,
            orchestrator_eligible: true,
            rotation_eligible: true,
            managed_pool_eligible: true,
        },
    }
}

fn environment(home: &Path) -> MapEnvironment {
    MapEnvironment::new([]).with_home(home)
}

fn grok_auth(email: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "xai::oidc": {
            "auth_mode": "oidc",
            "key": "fixture-token",
            "email": email
        }
    }))
    .unwrap()
}

#[test]
fn exact_email_selects_the_provider_profile() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let profile = home.join(".codex2");
    fs::create_dir_all(&profile).unwrap();
    let token = format!(
        "{}.{}.signature",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{}"#),
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"email":"Person@Example.Test"}"#)
    );
    fs::write(
        profile.join("auth.json"),
        serde_json::to_vec(&serde_json::json!({"tokens": {"id_token": token}})).unwrap(),
    )
    .unwrap();
    let profiles = vec![snapshot(Engine::Codex, "2", profile)];
    let selected = resolve_profile_selector(
        Engine::Codex,
        &profiles,
        "person@example.test",
        &home,
        &environment(&home),
        &RealFileSystem,
    )
    .unwrap();
    assert_eq!(selected.account, "2");
}

#[test]
fn duplicate_emails_fail_with_candidate_profile_ids() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let mut profiles = Vec::new();
    for account in ["2", "3"] {
        let profile = home.join(format!(".codex{account}"));
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join("auth.json"),
            serde_json::to_vec(&serde_json::json!({
                "tokens": {"id_token": format!(
                    "e30.{}.sig",
                    base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(br#"{"email":"person@example.test"}"#)
                )}
            }))
            .unwrap(),
        )
        .unwrap();
        profiles.push(snapshot(Engine::Codex, account, profile));
    }
    let error = resolve_profile_selector(
        Engine::Codex,
        &profiles,
        "PERSON@example.test",
        &home,
        &environment(&home),
        &RealFileSystem,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("matching profile IDs: 2, 3"), "{error}");
}

#[test]
fn profile_directory_aliases_are_explicit_and_portable() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let profile = home.join("codex-work");
    let profile = snapshot(Engine::Codex, "1", profile);
    let environment = environment(&home);
    assert!(profile_matches_selector(
        &profile,
        "alias:codex-work",
        &home,
        &environment,
        &RealFileSystem,
    ));
    assert!(!profile_matches_selector(
        &profile,
        "codex-work",
        &home,
        &environment,
        &RealFileSystem,
    ));
}

#[test]
fn selectors_are_provider_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let profile = home.join("grok");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("auth.json"), grok_auth("person@example.test")).unwrap();
    let profiles = vec![snapshot(Engine::Grok, "1", profile)];
    assert!(resolve_profile_selector(
        Engine::Codex,
        &profiles,
        "person@example.test",
        &home,
        &environment(&home),
        &RealFileSystem,
    )
    .is_err());
}
