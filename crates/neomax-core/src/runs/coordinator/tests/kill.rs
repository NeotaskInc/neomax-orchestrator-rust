use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::accounts::{AccountControlStore, AccountInventory, RotationClaimStore, SelectionPolicy};
use crate::orchestration::continuation::{
    ContinuationMode, ContinuationOutcome, ContinuationPort, ContinuationRequest,
    ContinuationService,
};
use crate::providers::{Provider, ProviderRegistry};
use crate::runs::coordinator::{AttemptRunner, RunCoordinator};
use crate::runs::lifecycle::{RunFinalizer, WorktreeFinalizer};
use crate::runs::{EventStore, HistoryStore, RunLiveWorkSource, RunRecord, RunStatus, RunStore};
use crate::usage::UsageCacheStore;
use crate::{Engine, Result, WorkerScope};

use super::fixture::{
    AttemptSequence, FixedClock, FixtureHandoff, FixtureRotation, NoLiveWorkers, ProviderFixture,
    profile, run,
};

struct KillAfterFirstAttempt<'a> {
    runs: &'a RunStore,
    calls: AtomicUsize,
}

impl AttemptRunner for KillAfterFirstAttempt<'_> {
    fn run_attempt(&self, run: &mut RunRecord) -> Result<RunStatus> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            self.runs.update(&run.id, |persisted| {
                persisted.killed = true;
                persisted.status = RunStatus::Aborted;
                persisted.interrupt_signal = Some(15);
                persisted.ended = Some(99);
                Ok(())
            })?;
            return Ok(RunStatus::Limit);
        }
        panic!("a persisted kill must prevent a retry");
    }
}

struct KillDuringContinuation<'a> {
    runs: &'a RunStore,
    calls: AtomicUsize,
}

struct ResumeWhileAttemptExits<'a> {
    runs: &'a RunStore,
    next_profile: std::path::PathBuf,
}

impl AttemptRunner for ResumeWhileAttemptExits<'_> {
    fn run_attempt(&self, run: &mut RunRecord) -> Result<RunStatus> {
        self.runs.update(&run.id, |persisted| {
            persisted.attempt = run.attempt + 1;
            persisted.profile = self.next_profile.clone();
            persisted.status = RunStatus::Done;
            persisted.worker_pid = None;
            Ok(())
        })?;
        Ok(RunStatus::Error)
    }
}

impl ContinuationPort for KillDuringContinuation<'_> {
    fn continue_after_limit(&self, request: &ContinuationRequest) -> Result<ContinuationOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.runs.update(&request.run_id, |persisted| {
            persisted.killed = true;
            persisted.status = RunStatus::Aborted;
            persisted.interrupt_signal = Some(15);
            persisted.ended = Some(99);
            Ok(())
        })?;
        Ok(ContinuationOutcome {
            mode: ContinuationMode::SameProviderHandoff,
            target_engine: request.target.engine,
            target_profile: request.target.profile.clone(),
            cooldown_profile: request.source_profile.clone(),
            resume_session: None,
            rotation_effects: None,
        })
    }
}

struct CoordinatorFixture {
    providers: ProviderRegistry,
    usage: UsageCacheStore,
    controls: AccountControlStore,
    runs: RunStore,
    events: EventStore,
    history: HistoryStore,
    claims: RotationClaimStore,
    scope: WorkerScope,
    first: std::path::PathBuf,
    second: std::path::PathBuf,
}

fn fixture(temp: &tempfile::TempDir) -> CoordinatorFixture {
    let first = profile(Engine::Claude, "1", temp.path());
    let second = profile(Engine::Claude, "2", temp.path());
    let providers = ProviderRegistry::new([Box::new(ProviderFixture {
        engine: Engine::Claude,
        profiles: vec![first.clone(), second.clone()],
    }) as Box<dyn Provider>]);
    let usage = UsageCacheStore::new(temp.path().join("usage"));
    let controls = AccountControlStore::new(
        temp.path().join("cooldown.json"),
        temp.path().join("paused.json"),
    );
    let runs = RunStore::new(temp.path().join("runs"));
    let events = EventStore::new(temp.path().join("events"));
    let history = HistoryStore::new(
        temp.path().join("history.db"),
        temp.path().join("logs"),
        temp.path().join("history-logs"),
        temp.path().join("history-pending"),
    );
    let claims = RotationClaimStore::new(
        temp.path().join("rotation-claims.json"),
        temp.path().join("rotation.lock"),
    );
    CoordinatorFixture {
        providers,
        usage,
        controls,
        runs,
        events,
        history,
        claims,
        scope: WorkerScope::all(),
        first: first.path,
        second: second.path,
    }
}

struct NoWorktree;

impl WorktreeFinalizer for NoWorktree {
    fn record_outcome(&self, _run: &mut RunRecord) -> Result<()> {
        Ok(())
    }
}

static NO_WORKTREE: NoWorktree = NoWorktree;

fn finalizer(fixture: &CoordinatorFixture) -> RunFinalizer<'_> {
    RunFinalizer {
        runs: &fixture.runs,
        events: &fixture.events,
        history: &fixture.history,
        controls: &fixture.controls,
        worktrees: &NO_WORKTREE,
        pull_requests: None,
    }
}

#[test]
fn persisted_kill_is_checked_before_a_new_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let live_work = RunLiveWorkSource::new(&fixture.runs, &NoLiveWorkers);
    let mut item = run(Engine::Claude, fixture.first.clone(), temp.path());
    item.killed = true;
    item.status = RunStatus::Aborted;
    item.interrupt_signal = Some(15);
    fixture.runs.create(&item).unwrap();
    let attempts = AttemptSequence::new([RunStatus::Done]);
    let finalizer = finalizer(&fixture);

    let result = RunCoordinator {
        attempts: &attempts,
        inventory: &AccountInventory {
            providers: &fixture.providers,
            quota: &fixture.usage,
            controls: &fixture.controls,
            live_work: &live_work,
        },
        runs: &fixture.runs,
        events: &fixture.events,
        controls: &fixture.controls,
        finalizer: &finalizer,
        scope: &fixture.scope,
        selection: SelectionPolicy::default(),
        clock: &FixedClock,
        default_cooldown: Duration::from_secs(1_800),
        continuation: None,
        claims: Some(&fixture.claims),
    }
    .execute(&mut item)
    .unwrap();

    assert_eq!(result.status, RunStatus::Aborted);
    assert!(attempts.observed().is_empty());
    assert_eq!(fixture.runs.load("run").unwrap().status, RunStatus::Aborted);
    assert!(fixture.runs.load("run").unwrap().killed);
    assert!(fixture
        .events
        .read(Some("run"), 0)
        .unwrap()
        .iter()
        .all(|event| !event.event.contains("attempt")));
}

#[test]
fn kill_after_limit_prevents_failover_and_retry() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let live_work = RunLiveWorkSource::new(&fixture.runs, &NoLiveWorkers);
    let mut item = run(Engine::Claude, fixture.first.clone(), temp.path());
    fixture.runs.create(&item).unwrap();
    let attempts = KillAfterFirstAttempt {
        runs: &fixture.runs,
        calls: AtomicUsize::new(0),
    };
    let rotation = FixtureRotation {
        calls: Arc::default(),
    };
    let handoff = FixtureHandoff::default();
    let continuation = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let finalizer = finalizer(&fixture);
    let result = RunCoordinator {
        attempts: &attempts,
        inventory: &AccountInventory {
            providers: &fixture.providers,
            quota: &fixture.usage,
            controls: &fixture.controls,
            live_work: &live_work,
        },
        runs: &fixture.runs,
        events: &fixture.events,
        controls: &fixture.controls,
        finalizer: &finalizer,
        scope: &fixture.scope,
        selection: SelectionPolicy::default(),
        clock: &FixedClock,
        default_cooldown: Duration::from_secs(1_800),
        continuation: Some(&continuation),
        claims: Some(&fixture.claims),
    }
    .execute(&mut item)
    .unwrap();

    assert_eq!(result.status, RunStatus::Aborted);
    assert_eq!(attempts.calls.load(Ordering::SeqCst), 1);
    assert!(rotation.calls.lock().unwrap().is_empty());
    assert!(handoff.batons.lock().unwrap().is_empty());
    assert!(fixture
        .events
        .read(Some("run"), 0)
        .unwrap()
        .iter()
        .all(|event| !event.event.contains("failover")));
}

#[test]
fn superseded_attempt_cannot_overwrite_or_refinalize_the_resumed_run() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let live_work = RunLiveWorkSource::new(&fixture.runs, &NoLiveWorkers);
    let mut item = run(Engine::Claude, fixture.first.clone(), temp.path());
    fixture.runs.create(&item).unwrap();
    let attempts = ResumeWhileAttemptExits {
        runs: &fixture.runs,
        next_profile: fixture.second.clone(),
    };
    let finalizer = finalizer(&fixture);

    let result = RunCoordinator {
        attempts: &attempts,
        inventory: &AccountInventory {
            providers: &fixture.providers,
            quota: &fixture.usage,
            controls: &fixture.controls,
            live_work: &live_work,
        },
        runs: &fixture.runs,
        events: &fixture.events,
        controls: &fixture.controls,
        finalizer: &finalizer,
        scope: &fixture.scope,
        selection: SelectionPolicy::default(),
        clock: &FixedClock,
        default_cooldown: Duration::from_secs(1_800),
        continuation: None,
        claims: Some(&fixture.claims),
    }
    .execute(&mut item)
    .unwrap();

    assert_eq!(result.status, RunStatus::Done);
    assert_eq!(result.exit_code, 0);
    assert_eq!(item.attempt, 2);
    assert_eq!(item.profile, fixture.second);
    let persisted = fixture.runs.load("run").unwrap();
    assert_eq!(persisted.status, RunStatus::Done);
    assert_eq!(persisted.attempt, 2);
    assert_eq!(persisted.profile, fixture.second);
    assert!(fixture.history.get("run").unwrap().is_none());
    assert!(
        fixture
            .events
            .read(Some("run"), 0)
            .unwrap()
            .iter()
            .all(|event| event.event != "finished")
    );
}

#[test]
fn kill_during_continuation_prevents_the_handoff_from_becoming_a_new_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = fixture(&temp);
    let live_work = RunLiveWorkSource::new(&fixture.runs, &NoLiveWorkers);
    let mut item = run(Engine::Claude, fixture.first.clone(), temp.path());
    fixture.runs.create(&item).unwrap();
    let attempts = AttemptSequence::new([RunStatus::Limit, RunStatus::Done]);
    let continuation = KillDuringContinuation {
        runs: &fixture.runs,
        calls: AtomicUsize::new(0),
    };
    let finalizer = finalizer(&fixture);
    let result = RunCoordinator {
        attempts: &attempts,
        inventory: &AccountInventory {
            providers: &fixture.providers,
            quota: &fixture.usage,
            controls: &fixture.controls,
            live_work: &live_work,
        },
        runs: &fixture.runs,
        events: &fixture.events,
        controls: &fixture.controls,
        finalizer: &finalizer,
        scope: &fixture.scope,
        selection: SelectionPolicy::default(),
        clock: &FixedClock,
        default_cooldown: Duration::from_secs(1_800),
        continuation: Some(&continuation),
        claims: Some(&fixture.claims),
    }
    .execute(&mut item)
    .unwrap();

    assert_eq!(result.status, RunStatus::Aborted);
    assert_eq!(attempts.observed().len(), 1);
    assert_eq!(continuation.calls.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.claims.claim_count(&fixture.second, 100.0), 0);
    assert!(fixture
        .events
        .read(Some("run"), 0)
        .unwrap()
        .iter()
        .all(|event| !event.event.contains("failover")));
}
