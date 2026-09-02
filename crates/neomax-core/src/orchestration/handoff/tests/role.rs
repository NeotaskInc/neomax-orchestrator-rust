use std::collections::BTreeMap;

#[cfg(windows)]
use std::path::{Path, PathBuf};

use super::super::{current_profile, infer_engine};
use crate::Engine;

fn env(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect()
}

#[test]
fn explicit_roles_cover_all_supported_providers() {
    let cases = [
        ("claude", Engine::Claude),
        ("codex", Engine::Codex),
        ("opencode", Engine::Opencode),
        ("kimi", Engine::Kimi),
        ("grok", Engine::Grok),
    ];
    for (role, expected) in cases {
        assert_eq!(infer_engine(&env(&[("NEOMAX_ROLE", role)])), expected);
    }
}

#[test]
fn fallback_order_matches_launcher_context() {
    assert_eq!(infer_engine(&env(&[("GROK_HOME", "/grok")])), Engine::Grok);
    assert_eq!(
        infer_engine(&env(&[("KIMI_CODE_HOME", "/kimi")])),
        Engine::Kimi
    );
    assert_eq!(
        infer_engine(&env(&[("CODEX_HOME", "/codex")])),
        Engine::Codex
    );
    assert_eq!(
        infer_engine(&env(&[
            ("CODEX_HOME", "/codex"),
            ("CLAUDE_CONFIG_DIR", "/claude"),
        ])),
        Engine::Claude
    );
    assert_eq!(infer_engine(&BTreeMap::new()), Engine::Claude);
}

#[test]
fn current_profile_uses_injected_paths_without_touching_the_host() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().join("workspace");
    let home = temp.path().join("home");
    let opencode_override = temp.path().join("profiles/.opencode-acct2");
    assert_eq!(
        current_profile(
            Engine::Codex,
            &env(&[("CODEX_HOME", "profiles/codex-2")]),
            &home,
            &cwd,
        ),
        cwd.join("profiles/codex-2")
    );
    assert_eq!(
        current_profile(Engine::Grok, &BTreeMap::new(), &home, &cwd),
        home.join(".grok")
    );
    assert_eq!(
        current_profile(
            Engine::Opencode,
            &env(&[(
                "XDG_DATA_HOME",
                opencode_override.to_string_lossy().as_ref(),
            )]),
            &home,
            &cwd,
        ),
        opencode_override
    );
    assert_eq!(
        current_profile(
            Engine::Claude,
            &env(&[("CLAUDE_CONFIG_DIR", "profiles/../claude")]),
            &home,
            &cwd,
        ),
        cwd.join("claude")
    );
}

#[cfg(windows)]
#[test]
fn rooted_or_drive_relative_profile_overrides_fail_closed_to_the_engine_default() {
    let cwd = Path::new(r"C:\workspace");
    let home = Path::new(r"C:\Users\tester");
    let expected = PathBuf::from(r"C:\Users\tester\.codex");

    assert_eq!(
        current_profile(
            Engine::Codex,
            &env(&[("CODEX_HOME", r"\profiles\secondary")]),
            home,
            cwd,
        ),
        expected
    );
    assert_eq!(
        current_profile(
            Engine::Codex,
            &env(&[("CODEX_HOME", r"C:profiles\secondary")]),
            home,
            cwd,
        ),
        expected
    );
}
