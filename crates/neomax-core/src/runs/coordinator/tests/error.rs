use std::sync::Arc;
use std::time::Duration;

use crate::accounts::{AccountControlStore, AccountInventory, RotationClaimStore, SelectionPolicy};
use crate::orchestration::continuation::{ContinuationPort, ContinuationService};
use crate::providers::{Provider, ProviderRegistry};
use crate::runs::coordinator::RunCoordinator;
use crate::runs::lifecycle::RunFinalizer;
use crate::runs::{EventStore, HistoryStore, RunLiveWorkSource, RunRecord, RunStatus, RunStore};
use crate::usage::UsageCacheStore;
use crate::{Engine, Result, WorkerScope};

use super::fixture::{
    profile, run, AttemptSequence, FixedClock, FixtureHandoff, FixtureRotation, NoLiveWorkers,
    ProviderFixture,
};

#[test]
fn generic_error_does_not_invoke_continuation_or_cross_provider() {
    let temp = tempfile::tempdir().unwrap();
    let claude = profile(Engine::Claude, "1", temp.path());
    let opencode = profile(Engine::Opencode, "1", temp.path());
    let providers = ProviderRegistry::new([
        Box::new(ProviderFixture {
            engine: Engine::Claude,
            profiles: vec![claude.clone()],
        }) as Box<dyn Provider>,
        Box::new(ProviderFixture {
            engine: Engine::Opencode,
            profiles: vec![opencode],
        }),
    ]);
    let usage = UsageCacheStore::new(temp.path().join("usage"));
    let controls = AccountControlStore::new(
        temp.path().join("cooldown.json"),
        temp.path().join("paused.json"),
    );
    let runs = RunStore::new(temp.path().join("runs"));
    let live_work = RunLiveWorkSource::new(&runs, &NoLiveWorkers);
    let events = EventStore::new(temp.path().join("events"));
    let history = HistoryStore::new(
        temp.path().join("history.db"),
        temp.path().join("logs"),
        temp.path().join("history-logs"),
        temp.path().join("history-pending"),
    );
    let no_worktree = |_run: &mut RunRecord| -> Result<()> { Ok(()) };
    let finalizer = RunFinalizer {
        runs: &runs,
        events: &events,
        history: &history,
        controls: &controls,
        worktrees: &no_worktree,
        pull_requests: None,
    };
    let inventory = AccountInventory {
        providers: &providers,
        quota: &usage,
        controls: &controls,
        live_work: &live_work,
    };
    let attempts = AttemptSequence::new([RunStatus::Error]);
    let rotation = FixtureRotation {
        calls: Arc::default(),
    };
    let handoff = FixtureHandoff::default();
    let continuation = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let claims = RotationClaimStore::new(
        temp.path().join("rotation-claims.json"),
        temp.path().join("rotation.lock"),
    );
    let scope = WorkerScope::all();
    let mut item = run(Engine::Claude, claude.path, temp.path());
    runs.create(&item).unwrap();
    let result = RunCoordinator {
        attempts: &attempts,
        inventory: &inventory,
        runs: &runs,
        events: &events,
        controls: &controls,
        finalizer: &finalizer,
        scope: &scope,
        selection: SelectionPolicy::default(),
        clock: &FixedClock,
        default_cooldown: Duration::from_secs(1_800),
        continuation: Some(&continuation as &dyn ContinuationPort),
        claims: Some(&claims),
    }
    .execute(&mut item)
    .unwrap();
    assert_eq!(result.status, RunStatus::Error);
    assert_eq!(item.engine, Engine::Claude);
    assert!(rotation.calls.lock().unwrap().is_empty());
    assert!(handoff.batons.lock().unwrap().is_empty());
    assert!(events
        .read(Some("run"), 0)
        .unwrap()
        .iter()
        .all(|event| !event.event.contains("failover")));
}
