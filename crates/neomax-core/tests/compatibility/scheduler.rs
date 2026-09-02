use std::fs;

use chrono::{DateTime, Utc};
use neomax_core::WorkerScope;
use neomax_core::scheduler::persistence::{
    PlanEvent, PlanEventStore, PlanRecord, PlanStatus, PlanStore,
};
use neomax_core::scheduler::{PartState, Plan, PlanState};

use super::support::{assert_fixture_is_sanitized, fixture_json, fixture_text};

#[test]
fn scheduler_plan_fixture_normalizes_dependencies_models_and_provider_order() {
    assert_fixture_is_sanitized("scheduler/plan.json");
    let expected = fixture_json("scheduler/plan.json");
    let plan = Plan::from_value(expected, &WorkerScope::all()).unwrap();
    assert_eq!(plan.plan_id.as_deref(), Some("compat-plan"));
    assert_eq!(plan.parts.len(), 5);
    assert_eq!(plan.parts[0].order, 0);
    assert_eq!(plan.parts[1].model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(plan.parts[2].model.as_deref(), Some("opencode/big-pickle"));
    assert_eq!(
        plan.graph()
            .unwrap()
            .ready_order(&PlanState::pending(&plan).states),
        ["p1"]
    );
    assert_eq!(plan.graph().unwrap().ids(), ["p1", "p2", "p3", "p4", "p5"]);
}

#[test]
fn scheduler_state_fixture_preserves_executions_and_ready_transitions() {
    let expected = fixture_json("scheduler/state.json");
    let state: PlanState = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(state.state("p1"), Some(PartState::Done));
    assert_eq!(state.state("p2"), Some(PartState::Running));
    assert_eq!(
        state.execution("p2").unwrap().run_id.as_deref(),
        Some("run-compat-001")
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), expected);

    let plan: Plan =
        Plan::from_value(fixture_json("scheduler/plan.json"), &WorkerScope::all()).unwrap();
    let graph = plan.graph().unwrap();
    assert!(state.ready(&graph).is_empty());
    let mut advanced = state.clone();
    advanced.set_state("p2", PartState::Done).unwrap();
    assert_eq!(advanced.ready(&graph), ["p4"]);
}

#[test]
fn scheduler_record_fixture_validates_and_plan_store_is_strict_on_missing_or_bad_state() {
    let expected = fixture_json("scheduler/record.json");
    let record: PlanRecord = serde_json::from_value(expected).unwrap();
    record.validate().unwrap();
    assert_eq!(record.status, PlanStatus::Running);
    assert_eq!(record.extra["future_plan_record_field"], "preserve");

    let temp = tempfile::tempdir().unwrap();
    let store = PlanStore::new(temp.path().join("plans"));
    store.create(&record).unwrap();
    let loaded = store.load("compat-plan").unwrap();
    assert_eq!(loaded.plan_id, "compat-plan");
    assert_eq!(loaded.state.state("p2"), Some(PartState::Running));
    assert!(store.all().unwrap().len() == 1);
    assert!(
        PlanStore::new(temp.path().join("missing"))
            .all()
            .unwrap()
            .is_empty()
    );

    let bad = temp.path().join("bad-plans");
    fs::create_dir_all(&bad).unwrap();
    fs::write(bad.join("bad.json"), "{").unwrap();
    let view = PlanStore::new(&bad).all_with_diagnostics().unwrap();
    assert!(view.records.is_empty());
    assert_eq!(view.diagnostics.len(), 1);
}

#[test]
fn scheduler_events_roundtrip_unknown_fields_and_reject_malformed_lines() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanEventStore::new(temp.path());
    let lines = fixture_text("scheduler/events.jsonl");
    let path = temp.path().join("scheduler/2026-08-23.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, lines).unwrap();
    let events = store.read(Some("compat-plan"), 0).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].extra["future_plan_event_field"], true);
    assert_eq!(events[2].part_id.as_deref(), Some("p2"));

    let invalid = PlanEvent::new("compat-plan", "updated", 1_787_488_123).unwrap();
    store
        .append(
            &invalid,
            DateTime::<Utc>::from_timestamp(1_787_488_123, 0).unwrap(),
        )
        .unwrap();
    let mut malformed = fs::read_to_string(&path).unwrap();
    malformed.push_str("not-json\n");
    fs::write(&path, malformed).unwrap();
    let view = store.read_with_diagnostics(Some("compat-plan"), 0).unwrap();
    assert_eq!(view.events.len(), 4);
    assert_eq!(view.diagnostics.len(), 1);
}
