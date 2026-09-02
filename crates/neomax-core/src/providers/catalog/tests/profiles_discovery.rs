use crate::providers::catalog::{
    discover_profile_snapshots, resolve_profile_path, AuthStatus, MapEnvironment, ProfileSelector,
};
use crate::Engine;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::symlink;

use super::super::fixtures;

fn canonical_expected(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut existing = path.to_path_buf();
    while !existing.exists() {
        missing.push(existing.file_name().unwrap().to_os_string());
        existing = existing.parent().unwrap().to_path_buf();
    }
    let mut resolved = std::fs::canonicalize(existing).unwrap();
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    resolved
}

#[test]
fn profile_discovery_supports_all_auth_shapes_without_returning_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let fs = fixtures::FixtureFs::default()
        .dir(home.join(".claude"))
        .dir(home.join(".claude-acct2"))
        .file(
            home.join(".claude-acct2").join(".credentials.json"),
            br#"{"accessToken":"secret"}"#,
        )
        .dir(home.join(".codex"))
        .file(
            home.join(".codex").join("auth.json"),
            br#"{"tokens":{"access_token":"secret"}}"#,
        )
        .dir(home.join(".opencode"))
        .file(
            fixtures::opencode_auth_path(home),
            br#"{"registry":{"key":"secret"}}"#,
        )
        .dir(home.join(".kimi-code"))
        .file(
            home.join(".kimi-code")
                .join("credentials")
                .join("kimi-code.json"),
            br#"{"refresh_token":"secret"}"#,
        )
        .dir(home.join(".grok"))
        .file(
            home.join(".grok").join("auth.json"),
            br#"{"oidc":{"auth_mode":"oidc","key":"secret"}}"#,
        );
    for engine in Engine::ALL {
        let profiles =
            discover_profile_snapshots(engine, &fixtures::environment(home), &fs).unwrap();
        assert!(profiles
            .iter()
            .any(|profile| profile.auth.is_authenticated()));
        assert!(profiles
            .iter()
            .any(|profile| matches!(profile.auth, AuthStatus::Authenticated { .. })));
        let debug = format!("{profiles:?}");
        assert!(!debug.contains("secret"));
    }
}

#[test]
fn profile_path_resolution_uses_configured_roots_without_consulting_real_home() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("fixture-home");
    let cwd = temp.path().join("workspace");
    let profiles = temp.path().join("profiles");
    let first = profiles.join("claude-one");
    let second = profiles.join("claude-two");
    let orchestrator = temp.path().join("claude-orchestrator");
    let environment = MapEnvironment::new([
        (
            "NEOMAX_PROFILES".into(),
            std::env::join_paths([&first, &second])
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
        (
            "NEOMAX_CLAUDE_ORCH".into(),
            orchestrator.to_string_lossy().into_owned(),
        ),
    ])
    .with_home(&home)
    .with_current_dir(&cwd);

    assert_eq!(
        resolve_profile_path(
            crate::Engine::Claude,
            ProfileSelector::Number(1),
            &environment
        )
        .unwrap(),
        canonical_expected(&first)
    );
    assert_eq!(
        resolve_profile_path(
            crate::Engine::Claude,
            ProfileSelector::Number(2),
            &environment
        )
        .unwrap(),
        canonical_expected(&second)
    );
    assert_eq!(
        resolve_profile_path(
            crate::Engine::Claude,
            ProfileSelector::Orchestrator,
            &environment
        )
        .unwrap(),
        canonical_expected(&orchestrator)
    );
    assert_eq!(
        resolve_profile_path(
            crate::Engine::Claude,
            ProfileSelector::Number(3),
            &environment
        )
        .unwrap(),
        canonical_expected(&first.join(".claude-acct3"))
    );
    let named_first = profiles.join(".claude-acct2");
    let named_second = profiles.join(".claude-acct3");
    let named_environment = MapEnvironment::new([(
        "NEOMAX_PROFILES".into(),
        std::env::join_paths([&named_first, &named_second])
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    )])
    .with_home(&home)
    .with_current_dir(&cwd);
    assert_eq!(
        resolve_profile_path(
            crate::Engine::Claude,
            ProfileSelector::Number(2),
            &named_environment
        )
        .unwrap(),
        canonical_expected(&named_first)
    );
    assert_eq!(
        resolve_profile_path(
            crate::Engine::Claude,
            ProfileSelector::Number(1),
            &named_environment
        )
        .unwrap(),
        canonical_expected(&profiles.join(".claude-acct1"))
    );
    assert!(!home.exists());
}

#[test]
fn profile_path_resolution_rejects_zero_and_empty_configured_roots() {
    let temp = tempfile::tempdir().unwrap();
    let environment = MapEnvironment::new([("NEOMAX_PROFILES".into(), String::new())])
        .with_home(temp.path().join("home"))
        .with_current_dir(temp.path());
    assert!(resolve_profile_path(
        crate::Engine::Claude,
        ProfileSelector::Number(0),
        &environment
    )
    .is_err());
    assert!(resolve_profile_path(
        crate::Engine::Claude,
        ProfileSelector::Number(1),
        &environment
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn explicit_symlink_roots_are_resolved_before_deriving_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("fixture-home");
    let real_root = temp.path().join("real-profiles");
    std::fs::create_dir_all(&real_root).unwrap();
    let linked_root = temp.path().join("linked-profiles");
    symlink(&real_root, &linked_root).unwrap();
    let environment = MapEnvironment::new([
        (
            "NEOMAX_PROFILES".into(),
            linked_root.to_string_lossy().into_owned(),
        ),
        (
            "NEOMAX_CLAUDE_ORCH".into(),
            linked_root.to_string_lossy().into_owned(),
        ),
    ])
    .with_home(&home)
    .with_current_dir(temp.path());

    assert_eq!(
        resolve_profile_path(Engine::Claude, ProfileSelector::Number(1), &environment).unwrap(),
        canonical_expected(&real_root)
    );
    assert_eq!(
        resolve_profile_path(Engine::Claude, ProfileSelector::Number(2), &environment).unwrap(),
        canonical_expected(&real_root.join(".claude-acct2"))
    );
    assert_eq!(
        resolve_profile_path(Engine::Claude, ProfileSelector::Orchestrator, &environment).unwrap(),
        canonical_expected(&real_root)
    );
}

#[cfg(unix)]
#[test]
fn derived_account_symlink_escape_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("fixture-home");
    let root = temp.path().join("profiles");
    let first = root.join(".claude-acct1");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, root.join(".claude-acct2")).unwrap();
    let environment = MapEnvironment::new([(
        "NEOMAX_PROFILES".into(),
        first.to_string_lossy().into_owned(),
    )])
    .with_home(&home)
    .with_current_dir(temp.path());

    assert!(
        resolve_profile_path(Engine::Claude, ProfileSelector::Number(2), &environment).is_err()
    );
}

#[test]
fn configured_profile_traversal_is_rejected_before_path_use() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("fixture-home");
    let traversing = temp.path().join("profiles").join("..").join("outside");
    let environment = MapEnvironment::new([
        (
            "NEOMAX_PROFILES".into(),
            traversing.to_string_lossy().into_owned(),
        ),
        (
            "NEOMAX_CLAUDE_ORCH".into(),
            traversing.to_string_lossy().into_owned(),
        ),
    ])
    .with_home(&home)
    .with_current_dir(temp.path());

    assert!(
        resolve_profile_path(Engine::Claude, ProfileSelector::Number(1), &environment).is_err()
    );
    assert!(
        resolve_profile_path(Engine::Claude, ProfileSelector::Orchestrator, &environment).is_err()
    );
    assert!(!temp.path().join("outside").exists());
}
