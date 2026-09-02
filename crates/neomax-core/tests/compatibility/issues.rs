use std::fs;

use neomax_core::issues::{
    Issue, IssueMirror, IssueStatus, IssueStore, MirrorState, PullRequestLink,
};

use super::support::{assert_fixture_is_sanitized, fixture_as, fixture_json, fixture_text};

#[test]
fn issue_fixture_preserves_unknown_fields_and_legacy_mirror_numbers() {
    assert_fixture_is_sanitized("issues/issue.json");
    let expected = fixture_json("issues/issue.json");
    let issue: Issue = serde_json::from_value(expected).unwrap();
    assert_eq!(issue.status, IssueStatus::Fixing);
    assert_eq!(issue.claim.as_ref().unwrap().pid, Some(321));
    assert_eq!(issue.repos["service-a"].number.as_deref(), Some("17"));
    assert_eq!(issue.repos["service-b"].number.as_deref(), Some("18"));
    assert_eq!(issue.repos["service-a"].state, MirrorState::Open);
    assert_eq!(issue.extra["future_issue_field"]["preserve"], true);
    assert_eq!(issue.history[0].extra["future_history_field"], 1);
}

#[test]
fn unknown_issue_status_roundtrips_without_being_dropped() {
    let issue: Issue = fixture_as("issues/unknown_status.json");
    assert_eq!(issue.status, IssueStatus::Unknown("provider_review".into()));
    assert_eq!(
        serde_json::to_value(issue).unwrap()["status"],
        "provider_review"
    );
}

#[test]
fn issue_store_handles_missing_state_and_skips_malformed_list_entries() {
    let temp = tempfile::tempdir().unwrap();
    let store = IssueStore::new(temp.path());
    assert!(store.load("ISSUE-1").unwrap().is_none());
    let issue: Issue = fixture_as("issues/issue.json");
    let path = temp.path().join("ISSUE-1.json");
    fs::write(&path, fixture_text("issues/issue.json")).unwrap();
    assert_eq!(store.load("ISSUE-1").unwrap().unwrap().key, issue.key);
    fs::write(temp.path().join("broken.json"), "{").unwrap();
    assert_eq!(store.list(None, None).unwrap().len(), 1);
}

#[test]
fn issue_transitions_and_links_preserve_durable_history() {
    let mut issue = Issue::new("ISSUE-3", "Portable issue", "project-a", 100);
    assert!(issue.link_run("run-compat-001"));
    assert!(!issue.link_run("run-compat-001"));
    issue.link_pull_request("service-a", "https://example.invalid/pull/3");
    issue.transition(IssueStatus::Claimed, 110).unwrap();
    issue.transition(IssueStatus::Fixing, 120).unwrap();
    issue.transition(IssueStatus::Done, 130).unwrap();
    assert_eq!(issue.runs, ["run-compat-001"]);
    assert_eq!(
        issue.pull_requests["service-a"],
        "https://example.invalid/pull/3"
    );
    assert!(issue.status.is_terminal());
}

#[test]
fn issue_schema_supports_number_string_and_unknown_mirror_fields() {
    let mirror: IssueMirror = serde_json::from_value(serde_json::json!({
        "number": 23,
        "state": "provider_state",
        "future": true
    }))
    .unwrap();
    assert_eq!(mirror.number.as_deref(), Some("23"));
    assert_eq!(mirror.state, MirrorState::Unknown("provider_state".into()));
    assert_eq!(mirror.extra["future"], true);
    let link: PullRequestLink = serde_json::from_value(serde_json::json!({
        "url": "https://example.invalid/pull/9",
        "future": "preserve"
    }))
    .unwrap();
    assert_eq!(link.extra["future"], "preserve");
}
