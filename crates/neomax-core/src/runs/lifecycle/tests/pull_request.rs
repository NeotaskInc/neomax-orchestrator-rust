use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};

use crate::git::pull_request::PullRequestRequest;
use crate::runs::lifecycle::{
    FinalizeOptions, PullRequestFinalizer, RunFinalizer, WorktreeFinalizer,
};
use crate::runs::{EventStore, HistoryStore, RunRecord, RunStatus, RunStore};
use crate::{Error, Result};

#[derive(Clone)]
struct FakePullRequest {
    calls: Arc<Mutex<usize>>,
    result: Option<String>,
    fail: bool,
}

impl PullRequestFinalizer for FakePullRequest {
    fn open(&self, _request: &PullRequestRequest) -> Result<Option<String>> {
        *self.calls.lock().unwrap() += 1;
        if self.fail {
            Err(Error::Message("gh unavailable".into()))
        } else {
            Ok(self.result.clone())
        }
    }
}

fn run(root: &std::path::Path) -> RunRecord {
    let mut run: RunRecord = serde_json::from_value(serde_json::json!({
        "id":"run", "engine":"codex", "model":"model", "prompt":"work",
        "profile":root.join("profiles/codex1"), "workdir":root,
        "repo":root, "worktree":root.join("worktree"), "branch":"feature/run",
        "base":"main", "status":"running", "started":1, "pr":true
    }))
    .unwrap();
    run.open_pull_request = true;
    run
}

fn fixture(
    root: &std::path::Path,
) -> (
    RunStore,
    EventStore,
    HistoryStore,
    crate::accounts::AccountControlStore,
) {
    (
        RunStore::new(root.join("runs")),
        EventStore::new(root.join("events")),
        HistoryStore::new(
            root.join("history.db"),
            root.join("logs"),
            root.join("history-logs"),
            root.join("history-pending"),
        ),
        crate::accounts::AccountControlStore::new(
            root.join("cooldown.json"),
            root.join("paused.json"),
        ),
    )
}

struct Changes;

impl WorktreeFinalizer for Changes {
    fn record_outcome(&self, run: &mut RunRecord) -> Result<()> {
        run.worktree_state = Some("has_changes".into());
        Ok(())
    }
}

static CHANGES: Changes = Changes;

struct NoChanges;

impl WorktreeFinalizer for NoChanges {
    fn record_outcome(&self, run: &mut RunRecord) -> Result<()> {
        run.worktree_state = Some("empty_kept".into());
        Ok(())
    }
}

static NO_CHANGES: NoChanges = NoChanges;

fn finalizer_with_worktree<'a>(
    runs: &'a RunStore,
    events: &'a EventStore,
    history: &'a HistoryStore,
    controls: &'a crate::accounts::AccountControlStore,
    worktrees: &'a dyn WorktreeFinalizer,
    pull_requests: Option<&'a dyn PullRequestFinalizer>,
) -> RunFinalizer<'a> {
    RunFinalizer {
        runs,
        events,
        history,
        controls,
        worktrees,
        pull_requests,
    }
}

fn finalizer<'a>(
    runs: &'a RunStore,
    events: &'a EventStore,
    history: &'a HistoryStore,
    controls: &'a crate::accounts::AccountControlStore,
    pull_requests: Option<&'a dyn PullRequestFinalizer>,
) -> RunFinalizer<'a> {
    finalizer_with_worktree(runs, events, history, controls, &CHANGES, pull_requests)
}

#[test]
fn opens_only_done_pr_runs_with_worktree_changes() {
    let temp = tempfile::tempdir().unwrap();
    let (runs, events, history, controls) = fixture(temp.path());
    let mut item = run(temp.path());
    runs.create(&item).unwrap();
    let calls = Arc::new(Mutex::new(0));
    let opener = FakePullRequest {
        calls: Arc::clone(&calls),
        result: Some("https://github.com/acme/repo/pull/1".into()),
        fail: false,
    };
    let finalizer = finalizer(&runs, &events, &history, &controls, Some(&opener));
    let result = finalizer
        .finish(
            &mut item,
            RunStatus::Done,
            FinalizeOptions::now(Utc.timestamp_opt(100, 0).unwrap()),
        )
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(
        runs.load("run").unwrap().pr_url.as_deref(),
        Some("https://github.com/acme/repo/pull/1")
    );
}

#[test]
fn pull_request_failure_is_a_warning_and_does_not_change_success() {
    let temp = tempfile::tempdir().unwrap();
    let (runs, events, history, controls) = fixture(temp.path());
    let mut item = run(temp.path());
    runs.create(&item).unwrap();
    let calls = Arc::new(Mutex::new(0));
    let opener = FakePullRequest {
        calls: Arc::clone(&calls),
        result: None,
        fail: true,
    };
    let finalizer = finalizer(&runs, &events, &history, &controls, Some(&opener));
    let result = finalizer
        .finish(
            &mut item,
            RunStatus::Done,
            FinalizeOptions::now(Utc.timestamp_opt(100, 0).unwrap()),
        )
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert!(result
        .warnings
        .iter()
        .any(|warning| warning.contains("pull request")));
    assert_eq!(runs.load("run").unwrap().status, RunStatus::Done);
    assert!(runs.load("run").unwrap().pr_url.is_none());
}

#[test]
fn pull_request_opening_is_gated_by_done_status_flag_and_changes() {
    let cases = [
        (RunStatus::Error, true, &CHANGES as &dyn WorktreeFinalizer),
        (RunStatus::Done, false, &CHANGES as &dyn WorktreeFinalizer),
        (RunStatus::Done, true, &NO_CHANGES as &dyn WorktreeFinalizer),
    ];
    for (status, open_pull_request, worktrees) in cases {
        let temp = tempfile::tempdir().unwrap();
        let (runs, events, history, controls) = fixture(temp.path());
        let mut item = run(temp.path());
        item.open_pull_request = open_pull_request;
        runs.create(&item).unwrap();
        let calls = Arc::new(Mutex::new(0));
        let opener = FakePullRequest {
            calls: Arc::clone(&calls),
            result: Some("https://github.com/acme/repo/pull/1".into()),
            fail: false,
        };
        let finalizer = finalizer_with_worktree(
            &runs,
            &events,
            &history,
            &controls,
            worktrees,
            Some(&opener),
        );
        finalizer
            .finish(
                &mut item,
                status,
                FinalizeOptions::now(Utc.timestamp_opt(100, 0).unwrap()),
            )
            .unwrap();
        assert_eq!(*calls.lock().unwrap(), 0);
    }
}
