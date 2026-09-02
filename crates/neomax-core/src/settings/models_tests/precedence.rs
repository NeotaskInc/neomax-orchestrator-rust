use std::collections::BTreeMap;

use super::super::{explicit_model_overrides, model_config_path, Engine, ModelOverrides};

#[test]
fn environment_models_are_used_when_config_does_not_override_them() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    let environment = BTreeMap::from([("NEOMAX_KIMI_MODEL".into(), "k2.7".into())]);
    let overrides = explicit_model_overrides(&config, &environment).unwrap();
    assert_eq!(
        overrides.get(&Engine::Kimi),
        Some(&"kimi-code/kimi-for-coding".to_string())
    );
}

#[test]
fn config_wins_over_environment_but_explicit_wins_over_config() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("config.toml");
    std::fs::write(model_config_path(&config), "grok = 'grok/configured'\n").unwrap();
    let environment = BTreeMap::from([("NEOMAX_GROK_MODEL".into(), "grok/environment".into())]);
    let settings = ModelOverrides::load(&model_config_path(&config)).unwrap();
    let configured = settings
        .effective_model_with_environment(Engine::Grok, None, &environment)
        .unwrap();
    assert_eq!(configured.model, "grok/configured");
    assert_eq!(configured.source, "config");
    let explicit = settings
        .effective_model_with_environment(Engine::Grok, Some("grok/argv"), &environment)
        .unwrap();
    assert_eq!(explicit.model, "grok/argv");
    assert_eq!(explicit.source, "argv");
}

#[test]
fn config_then_argv_precedence_is_preserved_for_all_provider_models() {
    let settings = ModelOverrides {
        claude: Some("claude/config".into()),
        codex: Some("terra".into()),
        opencode: Some("config/opencode".into()),
        kimi: Some("k2.7".into()),
        grok: Some("grok/config".into()),
        ..ModelOverrides::default()
    };
    let environment = BTreeMap::from([
        ("NEOMAX_CLAUDE_MODEL".into(), "claude/environment".into()),
        ("NEOMAX_CODEX_MODEL".into(), "luna".into()),
        (
            "NEOMAX_OPENCODE_MODEL".into(),
            "environment/opencode".into(),
        ),
        ("NEOMAX_KIMI_MODEL".into(), "k3".into()),
        ("NEOMAX_GROK_MODEL".into(), "grok/environment".into()),
    ]);
    let expected_config = [
        (Engine::Claude, "claude/config"),
        (Engine::Codex, "gpt-5.6-terra"),
        (Engine::Opencode, "config/opencode"),
        (Engine::Kimi, "kimi-code/kimi-for-coding"),
        (Engine::Grok, "grok/config"),
    ];
    for (engine, expected) in expected_config {
        let resolved = settings
            .effective_model_with_environment(engine, None, &environment)
            .unwrap();
        assert_eq!(resolved.model, expected);
        assert_eq!(resolved.source, "config");
    }

    let expected_argv = [
        (Engine::Claude, "claude/argv"),
        (Engine::Codex, "luna"),
        (Engine::Opencode, "argv/opencode"),
        (Engine::Kimi, "k3"),
        (Engine::Grok, "grok/argv"),
    ];
    for (engine, explicit) in expected_argv {
        let resolved = settings
            .effective_model_with_environment(engine, Some(explicit), &environment)
            .unwrap();
        let expected = match engine {
            Engine::Codex => "gpt-5.6-luna",
            Engine::Kimi => "kimi-code/k3",
            _ => explicit,
        };
        assert_eq!(resolved.model, expected);
        assert_eq!(resolved.source, "argv");
    }
}
