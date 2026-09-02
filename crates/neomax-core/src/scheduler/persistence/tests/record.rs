use super::super::PlanStatus;
use super::fixtures::record;

#[test]
fn record_contains_normalized_plan_metadata_and_pending_part_state() {
    let value = record("batch-1");
    assert_eq!(value.plan_id, "batch-1");
    assert_eq!(
        value.repository.as_deref().unwrap().to_str(),
        Some("/workspace/repository")
    );
    assert_eq!(value.plan.plan_id.as_deref(), Some("batch-1"));
    assert_eq!(value.status, PlanStatus::Pending);
    assert_eq!(value.state.states.len(), 2);
    assert!(value.state.executions.is_empty());
}

#[test]
fn record_round_trip_preserves_unknown_fields() {
    let mut value = record("batch-1");
    value.extra.insert("future_marker".into(), "kept".into());
    let encoded = serde_json::to_value(&value).unwrap();
    let restored: super::super::PlanRecord = serde_json::from_value(encoded).unwrap();
    assert_eq!(restored.extra.get("future_marker").unwrap(), "kept");
}

#[test]
fn mismatched_normalized_plan_id_is_rejected() {
    let mut value = record("batch-1");
    value.plan.plan_id = Some("other".into());
    assert!(value.validate().is_err());
}

#[test]
fn unknown_status_and_part_state_are_loaded_as_safe_forward_compatible_values() {
    let value = serde_json::json!({
        "plan_id": "batch-1",
        "repo": "/workspace/repository",
        "plan": {
            "plan": "batch-1",
            "repo": "/workspace/repository",
            "parts": [
                {"id": "first", "prompt": "first"},
                {"id": "second", "prompt": "second", "depends_on": ["first"]}
            ]
        },
        "state": {
            "states": {"first": "future_part_state", "second": "pending"},
            "executions": {}
        },
        "status": "future_plan_status",
        "created_at": 100,
        "updated_at": 100
    });
    let record: super::super::PlanRecord = serde_json::from_value(value).unwrap();
    assert_eq!(record.status, super::super::PlanStatus::Unknown);
    assert!(
        record
            .plan
            .parts
            .iter()
            .all(|part| part.engine == crate::Engine::Claude)
    );
    assert_eq!(
        record.state.state("first"),
        Some(crate::scheduler::PartState::Unknown)
    );
    assert!(record.validate().is_ok());
}
