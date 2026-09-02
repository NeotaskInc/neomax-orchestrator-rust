use std::time::Duration;

use chrono::{TimeZone, Utc};

use crate::accounts::AccountControlStore;
use crate::runs::lifecycle::WorktreeFinalizer;
use crate::runs::{ArchiveOutcome, EventStore, HistoryStore, RunRecord, RunStatus, RunStore};
use crate::{Error, Result};

use super::*;

mod pull_request;

fn run(root: &std::path::Path) -> RunRecord {
    serde_json::from_value(serde_json::json!({
        "id":"run", "engine":"codex", "model":"model", "prompt":"work",
        "profile":root.join("profiles/codex1"), "workdir":root,
        "status":"running", "started":1, "resets_at":500
    }))
    .unwrap()
}

struct Fixture {
    runs: RunStore,
    events: EventStore,
    history: HistoryStore,
    controls: AccountControlStore,
}

fn fixture(root: &std::path::Path) -> Fixture {
    Fixture {
        runs: RunStore::new(root.join("runs")),
        events: EventStore::new(root.join("events")),
        history: HistoryStore::new(
            root.join("history.db"),
            root.join("logs"),
            root.join("history-logs"),
            root.join("history-pending"),
        ),
        controls: AccountControlStore::new(root.join("cooldown.json"), root.join("paused.json")),
    }
}

#[test]
fn starts_an_attempt_with_durable_identity_fields() {
    let temp = tempfile::tempdir().unwrap();
    let mut item = run(temp.path());
    mark_attempt_started(&mut item, 42);
    assert_eq!(item.supervisor_pid, Some(42));
    assert_eq!(item.tried, [item.profile.clone()]);
    assert_eq!(item.status, RunStatus::Running);
}

#[test]
fn finalizes_cooldowns_archives_and_preserves_a_sticky_kill() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(temp.path());
    let mut persisted = run(temp.path());
    persisted.killed = true;
    fixture.runs.create(&persisted).unwrap();
    let mut item = run(temp.path());
    let worktrees = |run: &mut RunRecord| -> Result<()> {
        run.worktree_state = Some("empty_kept".into());
        Ok(())
    };
    let finalizer = RunFinalizer {
        runs: &fixture.runs,
        events: &fixture.events,
        history: &fixture.history,
        controls: &fixture.controls,
        worktrees: &worktrees,
        pull_requests: None,
    };
    let now = Utc.timestamp_opt(100, 0).unwrap();
    let outcome = finalizer
        .finish(
            &mut item,
            RunStatus::Limit,
            FinalizeOptions {
                now,
                account_number: Some(1),
                default_cooldown: Duration::from_secs(1_800),
            },
        )
        .unwrap();
    assert_eq!(outcome.exit_code, 75);
    assert_eq!(outcome.cooldown_until, Some(500.0));
    assert_eq!(outcome.archive, Some(ArchiveOutcome::Archived));
    assert!(outcome.warnings.is_empty());
    let saved = fixture.runs.load("run").unwrap();
    assert!(saved.killed);
    assert_eq!(saved.status, RunStatus::Limit);
    assert_eq!(saved.acknowledged, Some(false));
    assert!(fixture.history.get("run").unwrap().is_some());
    let events = fixture.events.read(Some("run"), 0).unwrap();
    assert_eq!(events[0].event, "finished");
    assert_eq!(events[0].extra["children"], 0);
}

struct BrokenWorktree;

impl WorktreeFinalizer for BrokenWorktree {
    fn record_outcome(&self, _run: &mut RunRecord) -> Result<()> {
        Err(Error::Message("inspection failed".into()))
    }
}

#[test]
fn ancillary_worktree_failure_does_not_erase_terminal_state() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(temp.path());
    let mut item = run(temp.path());
    fixture.runs.create(&item).unwrap();
    let finalizer = RunFinalizer {
        runs: &fixture.runs,
        events: &fixture.events,
        history: &fixture.history,
        controls: &fixture.controls,
        worktrees: &BrokenWorktree,
        pull_requests: None,
    };
    let outcome = finalizer
        .finish(
            &mut item,
            RunStatus::Error,
            FinalizeOptions::now(Utc.timestamp_opt(200, 0).unwrap()),
        )
        .unwrap();
    assert_eq!(outcome.exit_code, 1);
    assert_eq!(outcome.warnings.len(), 1);
    assert_eq!(fixture.runs.load("run").unwrap().status, RunStatus::Error);
}

#[test]
fn terminal_commit_preserves_a_concurrent_interruption_marker() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(temp.path());
    let mut persisted = run(temp.path());
    persisted.status = RunStatus::Aborted;
    persisted.killed = true;
    persisted.ended = Some(90);
    fixture.runs.create(&persisted).unwrap();

    let mut item = run(temp.path());
    let worktrees = |run: &mut RunRecord| -> Result<()> {
        assert_eq!(run.status, RunStatus::Aborted);
        assert!(run.killed);
        run.worktree_state = Some("empty_kept".into());
        Ok(())
    };
    let finalizer = RunFinalizer {
        runs: &fixture.runs,
        events: &fixture.events,
        history: &fixture.history,
        controls: &fixture.controls,
        worktrees: &worktrees,
        pull_requests: None,
    };

    let outcome = finalizer
        .finish(
            &mut item,
            RunStatus::Done,
            FinalizeOptions::now(Utc.timestamp_opt(200, 0).unwrap()),
        )
        .unwrap();

    assert_eq!(outcome.status, RunStatus::Aborted);
    let saved = fixture.runs.load("run").unwrap();
    assert_eq!(saved.status, RunStatus::Aborted);
    assert!(saved.killed);
    assert_eq!(saved.ended, Some(90));
}
