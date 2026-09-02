use serde_json::json;

use super::state::{PartState, PlanState};
use super::types::Plan;
use crate::WorkerScope;

#[test]
fn blocks_pending_parts_when_a_dependency_fails() {
    let plan = Plan::from_value(
        json!({"parts": [
            {"id": "first", "prompt": "first"},
            {"id": "second", "prompt": "second", "depends_on": ["first"]},
            {"id": "third", "prompt": "third", "depends_on": ["second"]}
        ]}),
        &WorkerScope::all(),
    )
    .unwrap();
    let graph = plan.graph().unwrap();
    let mut state = PlanState::pending(&plan);
    state.mark_failed("first").unwrap();
    assert_eq!(state.block_failed_dependencies(&graph), ["second"]);
    assert_eq!(state.state("second"), Some(PartState::Blocked));
    assert_eq!(state.block_failed_dependencies(&graph), ["third"]);
    assert!(state.finished());
}

#[test]
fn tracks_execution_metadata_and_live_counts() {
    let plan = Plan::from_value(
        json!({"parts": [{"id": "one", "prompt": "work"}]}),
        &WorkerScope::all(),
    )
    .unwrap();
    let mut state = PlanState::pending(&plan);
    state
        .mark_running(
            "one",
            "run-one",
            Some("neomax/run-one".into()),
            Some("acct-1".into()),
            12.5,
        )
        .unwrap();
    assert_eq!(state.live_count(), 1);
    assert_eq!(
        state.execution("one").unwrap().run_id.as_deref(),
        Some("run-one")
    );
    state.mark_done("one").unwrap();
    assert_eq!(state.live_count(), 0);
    assert!(state.finished());
}

#[test]
fn terminal_parts_cannot_be_rewritten_to_another_state() {
    let plan = Plan::from_value(
        json!({"parts": [{"id": "one", "prompt": "work"}]}),
        &WorkerScope::all(),
    )
    .unwrap();
    let mut state = PlanState::pending(&plan);
    state.mark_done("one").unwrap();
    let error = state.mark_failed("one").unwrap_err();
    assert!(error.to_string().contains("already done"));
}
