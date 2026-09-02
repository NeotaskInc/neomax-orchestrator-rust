use serde_json::json;

use crate::issues::{
    ci_sync_action, classify_ci_checks, evaluate_merge_gate, CiSyncAction, MergeGate, MergeInput,
    MergeState, NEOMAX_CI_WORKFLOW,
};

#[test]
fn classifies_modern_legacy_and_malformed_rollup_entries() {
    let rollup = vec![
        json!({"name":"unit","conclusion":"SUCCESS"}),
        json!({"name":"startup","conclusion":"STARTUP_FAILURE"}),
        json!({"name":"test","conclusion":"FAILURE"}),
        json!({"name":"running","status":"IN_PROGRESS"}),
        json!({"context":"legacy","state":"ERROR"}),
        json!("garbage"),
    ];
    let result = classify_ci_checks(&rollup);
    assert_eq!(result.nonrun, vec!["startup"]);
    assert_eq!(result.real_failures, vec!["test", "legacy"]);
    assert_eq!(result.pending, vec!["running"]);
}

#[test]
fn nonrunning_checks_are_ignored_only_when_requested() {
    let rollup = vec![json!({"name":"billing","conclusion":"ACTION_REQUIRED"})];
    let input = MergeInput {
        state: Some("OPEN"),
        merge_state: MergeState::Clean,
        url: Some("https://example.test/pr/1"),
        rollup: &rollup,
    };
    assert!(matches!(
        evaluate_merge_gate(&input, true),
        MergeGate::Ready { .. }
    ));
    assert!(matches!(
        evaluate_merge_gate(&input, false),
        MergeGate::Blocked(_)
    ));
}

#[test]
fn empty_unknown_rollup_waits_but_clean_rollup_is_ready() {
    let empty = Vec::new();
    let unknown = MergeInput {
        state: Some("OPEN"),
        merge_state: MergeState::Unknown("UNKNOWN".into()),
        url: None,
        rollup: &empty,
    };
    assert!(matches!(
        evaluate_merge_gate(&unknown, true),
        MergeGate::Waiting(_)
    ));
    let clean = MergeInput {
        merge_state: MergeState::Clean,
        ..unknown
    };
    assert!(matches!(
        evaluate_merge_gate(&clean, true),
        MergeGate::Ready { .. }
    ));
}

#[test]
fn workflow_sync_plan_protects_hand_edits() {
    assert_eq!(ci_sync_action(None, false), CiSyncAction::Create);
    assert_eq!(
        ci_sync_action(Some(NEOMAX_CI_WORKFLOW), false),
        CiSyncAction::Unchanged
    );
    assert_eq!(
        ci_sync_action(Some("name: human"), false),
        CiSyncAction::SkipHandEdited
    );
    assert_eq!(
        ci_sync_action(Some("name: human"), true),
        CiSyncAction::Update
    );
}
