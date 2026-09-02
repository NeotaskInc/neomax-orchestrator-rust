use serde_json::json;

use crate::shepherd::classify_ci_checks;

#[test]
fn classifies_check_run_and_legacy_status_context_shapes() {
    let result = classify_ci_checks(&[
        json!({"name":"unit", "conclusion":"failure"}),
        json!({"context":"legacy", "state":"error"}),
        json!({"name":"billing", "conclusion":"startup_failure"}),
        json!({"name":"queued", "status":"in_progress"}),
        json!({"name":"good", "conclusion":"success"}),
    ]);
    assert_eq!(result.real_failures, vec!["unit", "legacy"]);
    assert_eq!(result.non_running, vec!["billing"]);
    assert_eq!(result.pending, vec!["queued"]);
}

#[test]
fn malformed_rollup_entries_are_ignored_and_names_are_safe() {
    let result = classify_ci_checks(&[
        json!(null),
        json!("not a check"),
        json!({"name": 42, "conclusion":"failure"}),
        json!({"conclusion":"failure"}),
    ]);
    assert_eq!(result.real_failures, vec!["check", "check"]);
    assert_eq!(result.blocking_failures(true), vec!["check"]);
}

#[test]
fn shepherd_classification_matches_shared_issue_classification() {
    let rollup = vec![
        json!({"name":"unit", "conclusion":"failure"}),
        json!({"context":"legacy", "state":"error"}),
        json!({"name":"billing", "conclusion":"startup_failure"}),
        json!({"name":"queued", "status":"in_progress"}),
        json!({"name":"good", "conclusion":"success"}),
    ];
    let shared = crate::issues::classify_ci_checks(&rollup);
    let shepherd = classify_ci_checks(&rollup);
    assert_eq!(shepherd.real_failures, shared.real_failures);
    assert_eq!(shepherd.non_running, shared.nonrun);
    assert_eq!(shepherd.pending, shared.pending);
}

#[test]
fn normalized_whitespace_keeps_failures_fail_closed() {
    let rollup = vec![json!({
        "name": " ",
        "conclusion": " failure "
    })];
    let shared = crate::issues::classify_ci_checks(&rollup);
    let shepherd = classify_ci_checks(&rollup);
    assert_eq!(shared.real_failures, vec!["check"]);
    assert_eq!(shepherd.real_failures, shared.real_failures);
    assert!(shepherd.pending.is_empty());
}
