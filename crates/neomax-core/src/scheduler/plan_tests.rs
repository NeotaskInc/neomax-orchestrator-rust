use serde_json::json;

use super::types::Plan;
use super::types::PlanSpec;
use crate::{Engine, WorkerScope};

#[test]
fn normalizes_plan_metadata_and_defaults_provider_in_scope_order() {
    let plan = Plan::normalize(
        PlanSpec {
            repo: Some("repo".into()),
            base: Some("main".into()),
            integration_branch: Some("neomax/integration".into()),
            plan_id: Some("batch-1".into()),
            parts: vec![json!({"prompt": "do the work"})],
            extra: Default::default(),
        },
        &WorkerScope::all(),
    )
    .unwrap();
    assert_eq!(plan.parts[0].id, "p1");
    assert_eq!(plan.parts[0].engine, Engine::Claude);
    assert_eq!(plan.parts[0].model, None);
    assert_eq!(plan.plan_id.as_deref(), Some("batch-1"));

    let opencode_only = Plan::from_value(
        json!({"parts": [{"prompt": "inspect"}]}),
        &WorkerScope::only(Engine::Opencode),
    )
    .unwrap();
    assert_eq!(opencode_only.parts[0].engine, Engine::Opencode);

    let mixed = "codex+grok+kimi".parse::<WorkerScope>().unwrap();
    let mixed_plan = Plan::from_value(json!({"parts": [{"prompt": "inspect"}]}), &mixed).unwrap();
    assert_eq!(mixed_plan.parts[0].engine, Engine::Grok);
}

#[test]
fn normalizes_tolerant_area_and_dependency_inputs() {
    let plan = Plan::from_value(
        json!({
            "parts": [
                {"id": "one", "prompt": "one", "area": "src/a", "depends_on": ["", 2]},
                {"id": "2", "prompt": "two", "area": {"ignored": true}}
            ]
        }),
        &WorkerScope::all(),
    )
    .unwrap();
    assert_eq!(
        plan.parts[0].area.iter().collect::<Vec<_>>(),
        [&"src/a".to_string()]
    );
    assert_eq!(
        plan.parts[0].depends_on.iter().collect::<Vec<_>>(),
        [&"2".to_string()]
    );
    assert!(plan.parts[1].area.is_empty());
}

#[test]
fn durable_plan_round_trip_preserves_input_order() {
    let plan = Plan::from_value(
        json!({"parts": [
            {"id": "z", "prompt": "first"},
            {"id": "a", "prompt": "second"}
        ]}),
        &WorkerScope::all(),
    )
    .unwrap();
    let restored: Plan = serde_json::from_value(serde_json::to_value(&plan).unwrap()).unwrap();
    assert_eq!(restored.graph().unwrap().ids(), ["z", "a"]);
}

#[test]
fn future_plan_part_and_state_fields_survive_normalization_and_round_trip() {
    let plan = Plan::from_value(
        json!({
            "future_plan": {"enabled": true},
            "parts": [{
                "id": "one",
                "prompt": "work",
                "future_part": "preserve"
            }]
        }),
        &WorkerScope::all(),
    )
    .unwrap();
    assert_eq!(plan.extra["future_plan"]["enabled"], true);
    assert_eq!(plan.parts[0].extra["future_part"], "preserve");

    let mut state = super::state::PlanState::pending(&plan);
    state.extra.insert("future_state".into(), json!(42));
    state.executions.insert(
        "one".into(),
        super::state::PartExecution {
            extra: [("future_execution".into(), json!(true))]
                .into_iter()
                .collect(),
            ..Default::default()
        },
    );
    let restored: super::state::PlanState =
        serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    assert_eq!(restored.extra["future_state"], 42);
    assert_eq!(restored.executions["one"].extra["future_execution"], true);
}

#[test]
fn rejects_empty_plans_and_bad_part_ids() {
    let error = Plan::from_value(json!({"parts": []}), &WorkerScope::all()).unwrap_err();
    assert!(error.to_string().contains("no parts"));
    let error = Plan::from_value(json!({"parts": null}), &WorkerScope::all()).unwrap_err();
    assert!(error.to_string().contains("no parts"));
    let error = Plan::from_value(
        json!({"parts": [{"id": "bad/id", "prompt": "work"}]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid part id"));
    let error = Plan::from_value(
        json!({"parts": [{"id": {"bad": true}, "prompt": "work"}]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid part id"));
}
