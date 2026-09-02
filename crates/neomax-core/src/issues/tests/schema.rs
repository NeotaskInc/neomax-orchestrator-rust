use crate::issues::{Issue, IssueStatus};

#[test]
fn schema_round_trip_keeps_unknown_fields_and_reference_names() {
    let input = serde_json::json!({
        "key": "iss-1",
        "title": "a defect",
        "project": "demo",
        "status": "vendor-specific",
        "created": 10,
        "updated": 11,
        "repos": {"api": {"number": "7", "state": "open", "vendor": {"x": 1}}},
        "prs": {"api": "https://example.test/pr/1"},
        "vendor_field": {"preserve": true}
    });
    let issue: Issue = serde_json::from_value(input).unwrap();
    assert_eq!(issue.status, IssueStatus::Unknown("vendor-specific".into()));
    assert_eq!(issue.repos["api"].extra["vendor"]["x"], 1);
    let output = serde_json::to_value(issue).unwrap();
    assert_eq!(output["vendor_field"]["preserve"], true);
    assert_eq!(output["prs"]["api"], "https://example.test/pr/1");
    assert_eq!(output["repos"]["api"]["state"], "open");

    let numeric: Issue = serde_json::from_value(serde_json::json!({
        "repos": {"api": {"number": 7}}
    }))
    .unwrap();
    assert_eq!(numeric.repos["api"].number.as_deref(), Some("7"));
}

#[test]
fn status_transitions_allow_recovery() {
    let mut issue = Issue::new("iss-1", "title", "demo", 1);
    issue.transition(IssueStatus::Claimed, 2).unwrap();
    issue.transition(IssueStatus::Fixing, 3).unwrap();
    issue.transition(IssueStatus::Done, 4).unwrap();
    issue.transition(IssueStatus::Open, 5).unwrap();
    assert_eq!(issue.updated, 5);
}
