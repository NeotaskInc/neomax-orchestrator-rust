use std::fs;
use std::path::Path;

use super::super::prepare;

#[test]
fn preserves_profile_state_and_exposes_only_read_only_tools() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile with spaces - 日本語");
    let state = temp.path().join("state with spaces - Ελληνικά");
    fs::create_dir_all(profile.join("credentials")).unwrap();
    fs::create_dir_all(profile.join("sessions")).unwrap();
    fs::write(
        profile.join("credentials/kimi-code.json"),
        r#"{"refresh_token":"fixture-oauth-secret"}"#,
    )
    .unwrap();
    fs::write(
        profile.join("session_index.jsonl"),
        "fixture-session-index\n",
    )
    .unwrap();
    fs::write(
        profile.join("config.toml"),
        "[providers.\"managed:kimi-code\"]\ntype = \"kimi\"\napi_key = \"\"\n[providers.\"managed:kimi-code\".oauth]\nstorage = \"file\"\nkey = \"oauth/kimi-code\"\n\n[models.k3]\nprovider = \"kimi-code\"\n\n[tools]\ndisabled = [\"Read\"]\n",
    )
    .unwrap();
    let prepared = prepare(&profile, &state).unwrap();
    let config = fs::read_to_string(prepared.path().join("config.toml")).unwrap();
    assert_profile_state_link(
        &prepared.path().join("credentials"),
        &profile.join("credentials"),
    );
    assert_profile_state_link(&prepared.path().join("sessions"), &profile.join("sessions"));
    assert_eq!(
        fs::read_to_string(prepared.path().join("credentials/kimi-code.json")).unwrap(),
        r#"{"refresh_token":"fixture-oauth-secret"}"#
    );
    assert_eq!(
        fs::read_to_string(prepared.path().join("session_index.jsonl")).unwrap(),
        "fixture-session-index\n"
    );
    #[cfg(unix)]
    assert!(prepared.path().join("session_index.jsonl").is_symlink());
    #[cfg(windows)]
    {
        assert!(!prepared.path().join("session_index.jsonl").is_symlink());
        crate::io::verify_private_path(prepared.path()).unwrap();
        crate::io::verify_private_path(&prepared.path().join("config.toml")).unwrap();
        crate::io::verify_private_path(&prepared.path().join("session_index.jsonl")).unwrap();
    }
    assert!(config.contains("[models.k3]"));
    assert!(config.contains("\"Read\""));
    assert!(!config.contains("\"Write\""));
    assert!(!config.contains("\"Bash\""));
    let path = prepared.path().to_path_buf();
    drop(prepared);
    assert!(!path.exists());
    assert!(profile.join("credentials/kimi-code.json").exists());
}

fn assert_profile_state_link(staged: &Path, source: &Path) {
    #[cfg(unix)]
    assert!(staged.is_symlink());
    #[cfg(windows)]
    assert_eq!(
        fs::canonicalize(staged).unwrap(),
        fs::canonicalize(source).unwrap()
    );
    assert_eq!(
        fs::canonicalize(staged).unwrap(),
        fs::canonicalize(source).unwrap()
    );
}
