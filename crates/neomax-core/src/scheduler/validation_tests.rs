use serde_json::json;

use super::types::Plan;
use crate::{Engine, WorkerScope};

#[test]
fn validates_provider_specific_options() {
    for (engine, field) in [
        ("opencode", "effort"),
        ("kimi", "effort"),
        ("grok", "effort"),
    ] {
        let error = Plan::from_value(
            json!({"parts": [{"prompt": "work", "engine": engine, field: "high"}]}),
            &WorkerScope::all(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid"));
    }
    let error = Plan::from_value(
        json!({"parts": [{"prompt": "work", "engine": "codex", "effort": "max"}]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid"));
    let error = Plan::from_value(
        json!({"parts": [{"prompt": "work", "engine": "opencode", "ultra": true}]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("ultra"));
    let error = Plan::from_value(
        json!({"parts": [{"prompt": "work", "engine": "codex", "opus": true}]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("opus"));
}

#[test]
fn keeps_fable_default_and_requires_explicit_opus() {
    let plan = Plan::from_value(
        json!({"parts": [{"prompt": "work", "engine": "claude"}]}),
        &WorkerScope::all(),
    )
    .unwrap();
    assert_eq!(plan.parts[0].model, None);
    assert!(!plan.parts[0].opus);

    let plan = Plan::from_value(
        json!({"parts": [{"prompt": "work", "engine": "claude", "opus": true}]}),
        &WorkerScope::all(),
    )
    .unwrap();
    assert!(plan.parts[0].opus);
    assert_eq!(plan.parts[0].model, None);
}

#[test]
fn accepts_local_models_and_normalizes_legacy_provider_keys() {
    let plan = Plan::from_value(
        json!({
            "parts": [
                {"prompt": "claude", "engine": "claude", "model": "claude-local"},
                {"prompt": "codex", "engine": "codex", "codex_model": "gpt-5.6"},
                {"prompt": "kimi", "engine": "kimi", "kimi_model": "2.7"},
                {"prompt": "open", "engine": "opencode", "model": "local/big-pickle"},
                {"prompt": "grok", "engine": "grok", "model": "xai/local"}
            ]
        }),
        &WorkerScope::all(),
    )
    .unwrap();
    assert_eq!(plan.parts[0].model.as_deref(), Some("claude-local"));
    assert_eq!(plan.parts[1].codex_model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(
        plan.parts[2].kimi_model.as_deref(),
        Some("kimi-code/kimi-for-coding")
    );
    assert_eq!(plan.parts[3].model.as_deref(), Some("local/big-pickle"));
    assert_eq!(plan.parts[4].model.as_deref(), Some("xai/local"));
}

#[test]
fn rejects_wrong_legacy_keys_and_conflicting_model_fields() {
    let error = Plan::from_value(
        json!({"parts": [{"prompt": "work", "engine": "claude", "codex_model": "sol"}]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("codex_model"));
    let error = Plan::from_value(
        json!({"parts": [{"prompt": "work", "engine": "kimi", "kimi_model": "k3", "model": "local/kimi"}]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot combine"));
    let error = Plan::from_value(
        json!({"parts": [{"prompt": "work", "engine": "claude", "opus": true, "model": "claude-fable-5"}]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("different Claude model"));
}

#[test]
fn rejects_out_of_scope_engines() {
    let error = Plan::from_value(
        json!({"parts": [{"prompt": "work", "engine": "codex"}]}),
        &WorkerScope::only(Engine::Claude),
    )
    .unwrap_err();
    assert!(error.to_string().contains("out of fleet scope"));
}
