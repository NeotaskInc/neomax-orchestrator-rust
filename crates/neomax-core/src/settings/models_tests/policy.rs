use std::collections::BTreeMap;

use super::super::{Engine, ModelOverrides};

#[test]
fn policy_defaults_aliases_and_local_ids_are_resolved_for_every_provider() {
    let settings = ModelOverrides::default();
    let defaults = [
        (Engine::Claude, "claude-fable-5[1m]"),
        (Engine::Codex, "gpt-5.6-sol"),
        (Engine::Opencode, "opencode/big-pickle"),
        (Engine::Kimi, "kimi-code/k3"),
        (Engine::Grok, "grok-4.6"),
    ];
    for (engine, expected) in defaults {
        let resolved = settings
            .effective_model_with_environment(engine, None, &BTreeMap::new())
            .unwrap();
        assert_eq!(resolved.model, expected);
        assert_eq!(resolved.default, expected);
        assert_eq!(resolved.source, "default");
    }

    let environment = BTreeMap::from([
        ("NEOMAX_CLAUDE_MODEL".into(), "claude/local".into()),
        ("NEOMAX_CODEX_MODEL".into(), "terra".into()),
        (
            "NEOMAX_OPENCODE_MODEL".into(),
            "local/registry-model".into(),
        ),
        ("NEOMAX_KIMI_MODEL".into(), "k2.7".into()),
        ("NEOMAX_GROK_MODEL".into(), "grok/local".into()),
    ]);
    let expected_environment = [
        (Engine::Claude, "claude/local"),
        (Engine::Codex, "gpt-5.6-terra"),
        (Engine::Opencode, "local/registry-model"),
        (Engine::Kimi, "kimi-code/kimi-for-coding"),
        (Engine::Grok, "grok/local"),
    ];
    for (engine, expected) in expected_environment {
        let resolved = settings
            .effective_model_with_environment(engine, None, &environment)
            .unwrap();
        assert_eq!(resolved.model, expected);
        assert_eq!(resolved.source, "environment");
    }
}
