use crate::model::{PortalResponse, PortalSnapshot};

#[test]
fn status_shape_preserves_reference_top_level_fields() {
    let value = serde_json::to_value(PortalSnapshot {
        now: 10,
        inbox: 2,
        ..PortalSnapshot::default()
    })
    .unwrap();
    for key in [
        "now",
        "engines",
        "runs",
        "inbox",
        "ambient",
        "summary",
        "tasks",
        "projects",
        "queue",
        "usage",
        "orchestrators",
        "plans",
        "issues",
        "worktrees",
    ] {
        assert!(value.get(key).is_some(), "missing {key}");
    }
}

#[test]
fn response_tagging_is_explicit_for_non_status_payloads() {
    let value = PortalResponse::Json(serde_json::json!({"ok": true}))
        .into_json()
        .unwrap();
    assert_eq!(value["kind"], "Json");
    assert_eq!(value["data"]["ok"], true);
}
