use std::sync::Arc;
use std::thread;

use chrono::{DateTime, Datelike, Local, Utc};

use crate::issues::{
    ClaimLiveness, ClaimOwnerState, Issue, IssueStatus, IssueStore, IssueStoreConfig,
    ProcessLiveness,
};

#[derive(Debug)]
struct Probe {
    state: ClaimOwnerState,
    pid: bool,
}

impl ClaimLiveness for Probe {
    fn session_state(&self, _session: &str) -> ClaimOwnerState {
        self.state
    }
}

impl ProcessLiveness for Probe {
    fn pid_alive(&self, _pid: u32) -> bool {
        self.pid
    }
}

#[test]
fn saves_reference_shape_and_filters_without_losing_unknown_fields() {
    let temp = tempfile::tempdir().unwrap();
    let store = IssueStore::new(temp.path().join("issues"));
    let mut issue = Issue::new("iss-1", "title", "demo", 10);
    issue
        .extra
        .insert("vendor".into(), serde_json::json!({"x": 1}));
    issue
        .repos
        .insert("repo".into(), crate::issues::IssueMirror::local());
    store.save_at(&mut issue, 11).unwrap();
    assert_eq!(
        store
            .list(Some("demo"), Some(&IssueStatus::Open))
            .unwrap()
            .len(),
        1
    );
    let output = std::fs::read_to_string(temp.path().join("issues/iss-1.json")).unwrap();
    assert!(output.contains("\"vendor\""));
    assert!(output.contains("\"created\""));
    store.link_run_at("iss-1", "run-1", 12).unwrap();
    store.link_run_at("iss-1", "run-1", 13).unwrap();
    store
        .link_pull_request_at("iss-1", "repo", "https://example.test/pr/1", 14)
        .unwrap();
    store
        .set_status_at("iss-1", IssueStatus::Fixing, 15)
        .unwrap();
    let linked = store.load("iss-1").unwrap().unwrap();
    assert_eq!(linked.runs, vec!["run-1"]);
    assert_eq!(linked.pull_requests["repo"], "https://example.test/pr/1");
    assert_eq!(linked.status, IssueStatus::Fixing);
    assert_eq!(
        linked
            .history
            .iter()
            .filter(|event| event.event == "linked-run")
            .count(),
        1
    );
}

#[test]
fn claims_are_atomic_and_dead_claims_reclaim() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(IssueStore::new(temp.path()));
    let mut issue = Issue::new("iss-1", "title", "demo", 1);
    store.save_at(&mut issue, 1).unwrap();
    let live = Arc::new(Probe {
        state: ClaimOwnerState::Live,
        pid: false,
    });
    let mut joins = Vec::new();
    for index in 0..8 {
        let store = Arc::clone(&store);
        let probe = Arc::clone(&live);
        joins.push(thread::spawn(move || {
            store
                .claim(
                    "iss-1",
                    Some(format!("session-{index}")),
                    Some(index),
                    2,
                    probe.as_ref(),
                    probe.as_ref(),
                )
                .unwrap()
                .is_some()
        }));
    }
    let winners = joins
        .into_iter()
        .filter_map(|join| join.join().ok())
        .filter(|won| *won)
        .count();
    assert_eq!(winners, 1);
    let dead = Probe {
        state: ClaimOwnerState::Dead,
        pid: false,
    };
    assert!(store
        .claim("iss-1", Some("new".into()), Some(99), 3, &dead, &dead)
        .unwrap()
        .is_some());
    assert_eq!(
        store.release("iss-1", 4).unwrap().unwrap().status,
        IssueStatus::Open
    );
}

#[test]
fn malformed_files_are_skipped_by_list_and_direct_optional_load() {
    let temp = tempfile::tempdir().unwrap();
    let store = IssueStore::new(temp.path());
    let path = temp.path().join("bad.json");
    std::fs::write(&path, b"{").unwrap();
    assert!(store.load("bad").unwrap().is_none());
    let (issue, diagnostic) = store.load_with_diagnostic("bad").unwrap();
    assert!(issue.is_none());
    assert_eq!(diagnostic.unwrap().path, path);
    assert!(store.load_strict("bad").is_err());
    assert!(store.list(None, None).unwrap().is_empty());
    assert_eq!(store.list_with_diagnostics(None, None).unwrap().1.len(), 1);
}

#[test]
fn oversized_issue_load_is_absent_with_an_isolated_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let store = IssueStore::new(temp.path());
    let path = temp.path().join("oversized.json");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.set_len((crate::atomic::JSON_READ_MAX_BYTES as u64) + 1)
        .unwrap();
    assert!(store.load("oversized").unwrap().is_none());
    assert!(store.load_with_diagnostic("oversized").unwrap().1.is_some());
}

#[test]
fn malformed_issue_state_is_not_overwritten_without_explicit_repair() {
    let temp = tempfile::tempdir().unwrap();
    let store = IssueStore::new(temp.path());
    let path = temp.path().join("bad.json");
    std::fs::write(&path, b"{").unwrap();
    let mut replacement = Issue::new("bad", "replacement", "demo", 1);
    assert!(store.save_at(&mut replacement, 2).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"{");
    store.repair_at(&mut replacement, 3).unwrap();
    assert_eq!(store.load("bad").unwrap().unwrap().title, "replacement");
}

#[test]
fn audit_uses_the_event_timestamp_partition() {
    let temp = tempfile::tempdir().unwrap();
    let events = temp.path().join("events");
    let config = IssueStoreConfig {
        events_directory: Some(events.clone()),
        ..IssueStoreConfig::default()
    };
    let store = IssueStore::with_config(temp.path().join("issues"), config);
    let mut issue = Issue::new("iss-1", "title", "demo", 946684800);
    store.save_at(&mut issue, 946684800).unwrap();
    store
        .append_event_at("iss-1", "opened", Default::default(), 946684800)
        .unwrap();
    let day = DateTime::<Utc>::from_timestamp(946684800, 0)
        .unwrap()
        .with_timezone(&Local)
        .date_naive();
    let partition = events.join(format!(
        "{:04}-{:02}-{:02}.jsonl",
        day.year(),
        day.month(),
        day.day()
    ));
    assert!(partition.is_file());
    assert_eq!(
        std::fs::read_to_string(partition).unwrap().lines().count(),
        1
    );
}

#[test]
fn mutating_issue_paths_append_global_audit_events() {
    let temp = tempfile::tempdir().unwrap();
    let events = temp.path().join("events");
    let store = IssueStore::with_config(
        temp.path().join("issues"),
        IssueStoreConfig {
            events_directory: Some(events.clone()),
            ..IssueStoreConfig::default()
        },
    );
    let mut issue = Issue::new("iss-1", "title", "demo", 10);
    store.save_at(&mut issue, 10).unwrap();
    store
        .set_status_at("iss-1", IssueStatus::Fixing, 11)
        .unwrap();
    store.link_run_at("iss-1", "run-1", 12).unwrap();
    let mut lines = Vec::new();
    for entry in std::fs::read_dir(&events).unwrap().flatten() {
        let content = std::fs::read_to_string(entry.path()).unwrap();
        lines.extend(
            content
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok()),
        );
    }
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().any(|line| line["event"] == "status"));
    assert!(lines.iter().any(|line| line["event"] == "linked-run"));
}
