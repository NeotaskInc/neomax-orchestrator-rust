use super::*;
use std::fs;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

fn profile(home: &std::path::Path, directory: &str, email: &str) {
    let directory = home.join(directory);
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("auth.json"),
        serde_json::json!({
            "email": email, "account_id": directory.file_name().unwrap().to_str().unwrap(),
            "tokens": {"refresh_token": "fixture"}
        })
        .to_string(),
    )
    .unwrap();
}

#[test]
fn email_launches_the_same_numbered_account_and_preserves_forwarded_args() {
    let temp = tempfile::tempdir().unwrap();
    profile(temp.path(), ".codex", "first@example.test");
    profile(temp.path(), ".codex-acct2", "second@example.test");
    let env = MapEnvironment::default()
        .with_home(temp.path())
        .with_current_dir(temp.path());
    let helper = Launcher::AccountHelper(Engine::Codex);
    assert_eq!(
        normalize_with_environment(helper, &args(&["second@example.test", "--json"]), &env)
            .unwrap(),
        Some(args(&["run", "2", "--json"]))
    );
    assert_eq!(
        normalize_with_environment(
            Launcher::ProviderOrchestrator(Engine::Codex),
            &args(&["SECOND@example.test", "--dry-run"]),
            &env
        )
        .unwrap(),
        Some(args(&["2", "--dry-run"]))
    );
    assert_eq!(
        normalize_with_environment(
            Launcher::Universal,
            &args(&["--engine", "codex", "--account=second@example.test"]),
            &env
        )
        .unwrap(),
        Some(args(&["--engine", "codex", "--account=2"]))
    );
    assert_eq!(
        normalize_with_environment(helper, &args(&["run", "alias:.codex-acct2"]), &env).unwrap(),
        Some(args(&["run", "2"]))
    );
    assert!(
        normalize_with_environment(
            helper,
            &args(&["run", "2", "--", "person@example.test"]),
            &env
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn duplicate_or_unknown_email_never_falls_back_to_default_account() {
    let temp = tempfile::tempdir().unwrap();
    profile(temp.path(), ".codex", "same@example.test");
    profile(temp.path(), ".codex-acct2", "same@example.test");
    let env = MapEnvironment::default()
        .with_home(temp.path())
        .with_current_dir(temp.path());
    for email in ["same@example.test", "unknown@example.test"] {
        assert!(
            normalize_with_environment(
                Launcher::AccountHelper(Engine::Codex),
                &args(&[email]),
                &env
            )
            .is_err()
        );
    }
}
