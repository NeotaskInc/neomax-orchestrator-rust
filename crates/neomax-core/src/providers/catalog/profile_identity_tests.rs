use std::fs;

use super::*;
use crate::providers::catalog::{MapEnvironment, RealFileSystem};

#[test]
fn codex_email_is_read_from_jwt_without_exposing_identity_tokens() {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join(".codex2");
    fs::create_dir_all(&profile).unwrap();
    let token = format!(
        "{}.{}.signature",
        URL_SAFE_NO_PAD.encode(br#"{}"#),
        URL_SAFE_NO_PAD.encode(br#"{"email":"Person@Example.Test"}"#)
    );
    fs::write(
        profile.join("auth.json"),
        serde_json::to_vec(&serde_json::json!({
            "tokens": {"id_token": token}
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        profile_email(Engine::Codex, &profile, temp.path(), &RealFileSystem),
        Some("person@example.test".into())
    );
}

#[test]
fn malformed_email_metadata_is_ignored() {
    for value in [
        serde_json::json!({"email": "not-an-email"}),
        serde_json::json!({"email": "person@example.test extra"}),
        serde_json::json!({"email": "person@@example.test"}),
    ] {
        assert_eq!(first_email(&value), None);
    }
}

#[test]
fn opencode_reads_email_from_arbitrary_provider_entries() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let profile = home.join(".opencode2");
    let auth = profile.join("opencode/auth.json");
    fs::create_dir_all(auth.parent().unwrap()).unwrap();
    fs::write(
        &auth,
        serde_json::to_vec(&serde_json::json!({
            "openai": {
                "type": "oauth",
                "email": "Person@Example.Test",
                "access": "fixture-token"
            },
            "anthropic": {"type": "oauth", "access": "fixture-token"}
        }))
        .unwrap(),
    )
    .unwrap();
    let environment = MapEnvironment::new([]).with_home(&home);
    assert_eq!(
        profile_email_with_environment(
            Engine::Opencode,
            &profile,
            &home,
            &environment,
            &RealFileSystem,
        ),
        Some("person@example.test".into())
    );
}

#[test]
fn opencode_does_not_choose_between_distinct_provider_emails() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let profile = home.join(".opencode2");
    let auth = profile.join("opencode/auth.json");
    fs::create_dir_all(auth.parent().unwrap()).unwrap();
    fs::write(
        &auth,
        serde_json::to_vec(&serde_json::json!({
            "openai": {"email": "one@example.test"},
            "anthropic": {"email": "two@example.test"}
        }))
        .unwrap(),
    )
    .unwrap();
    let environment = MapEnvironment::new([]).with_home(&home);
    assert_eq!(
        profile_email_with_environment(
            Engine::Opencode,
            &profile,
            &home,
            &environment,
            &RealFileSystem,
        ),
        None
    );
}
