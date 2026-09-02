use std::collections::BTreeMap;

use super::*;

#[test]
fn parse_registry_options_accepts_inline_and_separate_values() {
    let parsed = parse_options(&[
        "--session=abc".into(),
        "--pid".into(),
        "42".into(),
        "--engine".into(),
        "opencode".into(),
        "--dir".into(),
        ".opencode-acct2".into(),
        "--reserved".into(),
    ])
    .unwrap();
    assert_eq!(parsed.session.as_deref(), Some("abc"));
    assert_eq!(parsed.pid, Some(42));
    assert_eq!(parsed.engine, Some(Engine::Opencode));
    assert_eq!(parsed.directory, Some(PathBuf::from(".opencode-acct2")));
    assert!(parsed.reserved);
}

#[test]
fn missing_session_fails_closed() {
    let error = parse_options(&["--pid".into(), "1".into()]).unwrap();
    assert!(error.session.is_none());
}

#[test]
fn registration_model_uses_cli_then_config_then_environment_then_default() {
    let overrides = ModelOverrides {
        codex: Some("terra".into()),
        ..ModelOverrides::default()
    };
    let environment = BTreeMap::from([("NEOMAX_CODEX_MODEL".into(), "gpt-5.6-luna".into())]);

    assert_eq!(
        registration_model(Engine::Codex, None, &overrides, &environment).unwrap(),
        "gpt-5.6-terra"
    );
    assert_eq!(
        registration_model(Engine::Codex, Some("sol"), &overrides, &environment,).unwrap(),
        "gpt-5.6-sol"
    );
    assert_eq!(
        registration_model(
            Engine::Grok,
            None,
            &ModelOverrides::default(),
            &BTreeMap::from([(
                String::from("NEOMAX_GROK_MODEL"),
                String::from("grok/local")
            )]),
        )
        .unwrap(),
        "grok/local"
    );
    assert_eq!(
        registration_model(
            Engine::Kimi,
            None,
            &ModelOverrides::default(),
            &BTreeMap::new(),
        )
        .unwrap(),
        "kimi-code/k3"
    );
}

#[cfg(windows)]
#[test]
fn rejects_windows_partial_root_profile_paths_before_home_joining() {
    let home = PathBuf::from(r"C:\Users\fixture");
    for raw in [r"\rooted", r"C:drive-relative"] {
        let error = absolute_path(PathBuf::from(raw), &home).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("rooted without an absolute prefix")
        );
    }
    assert_eq!(
        absolute_path(PathBuf::from(r"C:\profiles\claude-1"), &home).unwrap(),
        PathBuf::from(r"C:\profiles\claude-1")
    );
}
