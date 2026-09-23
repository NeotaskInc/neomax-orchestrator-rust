use super::super::{
    default_model_id, resolve_model, MapEnvironment, ModelDefaults, ModelOrigin,
    CLAUDE_DEFAULT_MODEL, CLAUDE_OPUS_MODEL, CLAUDE_OPUS_MODEL_1M,
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
fn opus_5_5_is_the_claude_default_and_other_claude_models_stay_explicit() {
    assert_eq!(default_model_id(Engine::Claude), CLAUDE_DEFAULT_MODEL);
    assert_eq!(CLAUDE_DEFAULT_MODEL, "claude-opus-5-5[1m]");
    assert_eq!(CLAUDE_DEFAULT_MODEL, CLAUDE_OPUS_MODEL_1M);
    assert_eq!(CLAUDE_OPUS_MODEL, "claude-opus-5-5");
    let implicit = resolve_model(Engine::Claude, None, &MapEnvironment::default()).unwrap();
    assert_eq!(implicit.id, "claude-opus-5-5[1m]");
    assert_eq!(implicit.origin, ModelOrigin::StrictDefault);
    for model in [
        CLAUDE_OPUS_MODEL,
        "claude-opus-5",
        "claude-opus-5[1m]",
        "claude-fable-5-1[1m]",
        "claude-fable-5",
    ] {
        let resolved = resolve_model(Engine::Claude, Some(model), &MapEnvironment::default())
            .unwrap();
        assert_eq!(resolved.id, model);
        assert_eq!(resolved.origin, ModelOrigin::Explicit);
    }
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
        ("sol", "gpt-5.6-sol"),
        ("terra", "gpt-5.6-terra"),
        ("luna", "gpt-5.6-luna"),
    ] {
        assert_eq!(resolve_model(Engine::Codex, Some(alias), &environment).unwrap().id, model);
        assert_eq!(resolve_model(Engine::Codex, Some(model), &environment).unwrap().id, model);
    }
}
