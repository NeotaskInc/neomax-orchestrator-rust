use serde_json::json;

use super::state::{PartState, PlanState};
use super::types::Plan;
use crate::WorkerScope;

#[test]
fn rejects_unknown_dependencies_and_cycles() {
    let error = Plan::from_value(
        json!({"parts": [{"id": "one", "prompt": "work", "depends_on": ["missing"]}]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown part"));

    let error = Plan::from_value(
        json!({"parts": [
            {"id": "one", "prompt": "one", "depends_on": ["two"]},
            {"id": "two", "prompt": "two", "depends_on": ["one"]}
        ]}),
        &WorkerScope::all(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("CYCLE"));
}

#[test]
fn ready_order_prioritizes_parts_with_more_direct_dependents() {
    let plan = Plan::from_value(
        json!({"parts": [
            {"id": "first", "prompt": "first"},
            {"id": "second", "prompt": "second"},
            {"id": "third", "prompt": "third", "depends_on": ["first"]},
            {"id": "fourth", "prompt": "fourth", "depends_on": ["first", "second"]},
            {"id": "fifth", "prompt": "fifth", "depends_on": ["second"]}
        ]}),
        &WorkerScope::all(),
    )
    .unwrap();
    let graph = plan.graph().unwrap();
    let state = PlanState::pending(&plan);
    assert_eq!(graph.ready_order(&state.states), ["first", "second"]);
    assert_eq!(graph.direct_dependent_count("first"), 2);
    assert_eq!(graph.direct_dependent_count("second"), 2);
}

#[test]
fn dependency_ready_requires_done_not_just_terminal() {
    let plan = Plan::from_value(
        json!({"parts": [
            {"id": "first", "prompt": "first"},
            {"id": "second", "prompt": "second", "depends_on": ["first"]}
        ]}),
        &WorkerScope::all(),
    )
    .unwrap();
    let graph = plan.graph().unwrap();
    let mut states = PlanState::pending(&plan).states;
    states.insert("first".into(), PartState::Failed);
    assert!(!graph.ready_order(&states).contains(&"first".to_string()));
    assert!(!graph.ready_order(&states).contains(&"second".to_string()));
    states.insert("first".into(), PartState::Done);
    assert_eq!(graph.ready_order(&states), ["second"]);
}
