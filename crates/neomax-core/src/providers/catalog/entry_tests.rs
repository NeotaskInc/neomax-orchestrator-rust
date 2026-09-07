use super::super::{MapEnvironment, RealFileSystem, inspect_profile_snapshot};
use super::*;

fn environment(home: &std::path::Path) -> MapEnvironment {
    MapEnvironment::new([])
        .with_home(home)
        .with_current_dir(home)
}

fn codex_profile(home: &std::path::Path, account: u32, email: &str) -> ProfileSnapshot {
    use base64::Engine as _;
    let env = environment(home);
    let path = resolve_profile_path(Engine::Codex, ProfileSelector::Number(account), &env).unwrap();
    std::fs::create_dir_all(&path).unwrap();
    let token = format!(
        "e30.{}.sig",
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::json!({"email":email}).to_string())
    );
    std::fs::write(
        path.join("auth.json"),
        serde_json::json!({"tokens":{"access_token":"fixture-token", "id_token":token}})
            .to_string(),
    )
    .unwrap();
    inspect_profile_snapshot(
        Engine::Codex,
        account.to_string(),
        path,
        false,
        home,
        &RealFileSystem,
    )
}

#[test]
fn explicit_numbers_can_initialize_every_provider_without_touching_disk() {
    let temp = tempfile::tempdir().unwrap();
    for engine in Engine::ALL {
        let entry = resolve_profile_entry(
            engine,
            &[],
            Some("2"),
            &environment(temp.path()),
            &RealFileSystem,
        )
        .unwrap()
        .unwrap();
        assert_eq!(entry.account, "2");
        assert!(entry.login_required);
        assert!(!entry.path.exists());
    }
}

#[test]
fn authenticated_accounts_launch_without_login_and_automatic_selection_keeps_its_policy() {
    let temp = tempfile::tempdir().unwrap();
    let profiles = vec![codex_profile(temp.path(), 2, "person@example.test")];
    assert!(profiles[0].eligibility.authenticated);
    let env = environment(temp.path());
    for selector in [
        "2",
        "PERSON@example.test",
        "alias:.codex-acct2",
        ".codex-acct2",
    ] {
        let entry = resolve_profile_entry(
            Engine::Codex,
            &profiles,
            Some(selector),
            &env,
            &RealFileSystem,
        )
        .unwrap()
        .unwrap();
        assert_eq!(entry.account, "2");
        assert!(!entry.login_required);
        verify_profile_entry_identity(Engine::Codex, &entry, &env, &RealFileSystem).unwrap();
    }
    assert!(
        resolve_profile_entry(Engine::Codex, &profiles, None, &env, &RealFileSystem)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_new_email_gets_an_unused_directory_and_never_an_existing_account() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path());
    let profiles = vec![codex_profile(temp.path(), 1, "existing@example.test")];
    let occupied = resolve_profile_path(Engine::Codex, ProfileSelector::Number(2), &env).unwrap();
    std::fs::create_dir(&occupied).unwrap();
    let entry = resolve_profile_entry(
        Engine::Codex,
        &profiles,
        Some("new@example.test"),
        &env,
        &RealFileSystem,
    )
    .unwrap()
    .unwrap();
    assert_eq!(entry.account, "3");
    assert!(entry.create_new);
    assert!(!entry.path.exists());
    assert!(verify_profile_entry_identity(Engine::Codex, &entry, &env, &RealFileSystem).is_err());
    codex_profile(temp.path(), 3, "wrong@example.test");
    assert!(verify_profile_entry_identity(Engine::Codex, &entry, &env, &RealFileSystem).is_err());
    codex_profile(temp.path(), 3, "new@example.test");
    verify_profile_entry_identity(Engine::Codex, &entry, &env, &RealFileSystem).unwrap();
}

#[test]
fn ambiguous_email_and_invalid_selectors_do_not_allocate_an_account() {
    let temp = tempfile::tempdir().unwrap();
    let profiles = vec![
        codex_profile(temp.path(), 1, "same@example.test"),
        codex_profile(temp.path(), 2, "same@example.test"),
    ];
    let env = environment(temp.path());
    for selector in [
        "same@example.test",
        "0",
        "../outside",
        "not-an-email@",
        "missing-alias",
    ] {
        assert!(
            resolve_profile_entry(
                Engine::Codex,
                &profiles,
                Some(selector),
                &env,
                &RealFileSystem
            )
            .is_err(),
            "{selector}"
        );
    }
}

#[test]
fn a_known_email_with_missing_auth_requests_login_in_its_original_profile() {
    let temp = tempfile::tempdir().unwrap();
    let env = environment(temp.path());
    let path = resolve_profile_path(Engine::Claude, ProfileSelector::Number(2), &env).unwrap();
    std::fs::create_dir(&path).unwrap();
    std::fs::write(
        path.join("settings.json"),
        r#"{"email":"known@example.test"}"#,
    )
    .unwrap();
    let profile = inspect_profile_snapshot(
        Engine::Claude,
        "2",
        path.clone(),
        false,
        temp.path(),
        &RealFileSystem,
    );
    let entry = resolve_profile_entry(
        Engine::Claude,
        &[profile],
        Some("known@example.test"),
        &env,
        &RealFileSystem,
    )
    .unwrap()
    .unwrap();
    assert_eq!(entry.path, path);
    assert!(entry.login_required);
    assert!(!entry.create_new);
}
