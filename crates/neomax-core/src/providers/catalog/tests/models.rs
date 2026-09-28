use super::super::{
    default_model_id, resolve_model, MapEnvironment, ModelDefaults, ModelOrigin,
    CLAUDE_DEFAULT_MODEL, CLAUDE_OPUS_MODEL,
};
use crate::Engine;

#[test]
fn compatibility_model_defaults_are_derived_from_the_catalog() {
    let defaults = ModelDefaults::default();
    assert_eq!(defaults, ModelDefaults::from_catalog());
    for engine in Engine::ALL {
        assert_eq!(defaults.for_engine(engine), default_model_id(engine));
    }
}

#[test]
fn opus_55_is_default_and_legacy_opus_remains_explicit() {
    assert_eq!(default_model_id(Engine::Claude), "claude-opus-5-5[1m]");
    assert_eq!(CLAUDE_DEFAULT_MODEL, "claude-opus-5-5[1m]");
    assert_ne!(default_model_id(Engine::Claude), CLAUDE_OPUS_MODEL);
    assert_eq!(
        resolve_model(
            Engine::Claude,
            Some(CLAUDE_OPUS_MODEL),
            &MapEnvironment::default()
        )
        .unwrap()
        .id,
        CLAUDE_OPUS_MODEL
    );
}

#[test]
fn model_precedence_keeps_strict_defaults_and_passes_local_ids() {
    let temp = tempfile::tempdir().unwrap();
    let environment = super::fixtures::environment(temp.path());
    assert_eq!(
        resolve_model(Engine::Claude, None, &environment)
            .unwrap()
            .id,
        "claude-opus-5-5[1m]"
    );
    let environment = MapEnvironment::new([
        ("NEOMAX_DEFAULT_MODEL".into(), "claude-local".into()),
        ("NEOMAX_CLAUDE_MODEL".into(), "claude-provider-local".into()),
    ])
    .with_home(temp.path());
    let resolved = resolve_model(Engine::Claude, Some("claude-explicit"), &environment).unwrap();
    assert_eq!(resolved.id, "claude-explicit");
    assert_eq!(resolved.origin, ModelOrigin::Explicit);
    assert_eq!(
        resolve_model(Engine::Claude, Some(CLAUDE_OPUS_MODEL), &environment)
            .unwrap()
            .id,
        CLAUDE_OPUS_MODEL
    );
    assert_eq!(
        resolve_model(Engine::Claude, None, &environment)
            .unwrap()
            .id,
        "claude-provider-local"
    );
    let environment = MapEnvironment::new([("NEOMAX_DEFAULT_MODEL".into(), "claude-local".into())])
        .with_home(temp.path());
    assert_eq!(
        resolve_model(Engine::Claude, None, &environment)
            .unwrap()
            .id,
        "claude-local[1m]"
    );
    assert_eq!(
        resolve_model(
            Engine::Opencode,
            Some("local-registry/big-pickle"),
            &environment
        )
        .unwrap()
        .id,
        "local-registry/big-pickle"
    );
    assert_eq!(
        resolve_model(Engine::Kimi, Some("k2.7"), &environment)
            .unwrap()
            .id,
        "kimi-code/kimi-for-coding"
    );
    assert!(resolve_model(Engine::Opencode, Some("big-pickle"), &environment).is_err());
}

#[test]
fn astra_is_default_and_worker_family_aliases_remain_available() {
    let environment = MapEnvironment::default();
    assert_eq!(default_model_id(Engine::Codex), "gpt-6-astra");
    for (alias, model) in [
        ("astra", "gpt-6-astra"),
        ("sol", "gpt-6-sol"),
        ("terra", "gpt-5.6-terra"),
        ("luna", "gpt-6-luna"),
    ] {
        assert_eq!(resolve_model(Engine::Codex, Some(alias), &environment).unwrap().id, model);
        assert_eq!(resolve_model(Engine::Codex, Some(model), &environment).unwrap().id, model);
    }
}

#[test]
fn new_models_and_legacy_ids_remain_explicitly_selectable() {
    let environment = MapEnvironment::default();
    for model in ["gpt-6-sol", "gpt-6-luna", "gpt-5.6-sol", "gpt-5.6-luna"] {
        assert_eq!(resolve_model(Engine::Codex, Some(model), &environment).unwrap().id, model);
        assert!(super::super::CODEX_SUBAGENT_MODELS.contains(&model));
    }
    assert_eq!(crate::models::codex_model_tier("gpt-6-sol"), Some("sol"));
    assert_eq!(crate::models::codex_model_tier("gpt-6-luna"), Some("luna"));
    assert_eq!(resolve_model(Engine::Codex, Some("gpt-5.6"), &environment).unwrap().id, "gpt-5.6-sol");
    assert_eq!(resolve_model(Engine::Claude, Some("claude-opus-5-5"), &environment).unwrap().id, "claude-opus-5-5");
}
