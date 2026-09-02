use std::time::Duration;

use crate::accounts::{AccountControlStore, AccountInventory, SelectionPolicy};
use crate::providers::{Provider, ProviderRegistry};
use crate::runs::coordinator::RunCoordinator;
use crate::runs::lifecycle::RunFinalizer;
use crate::runs::{EventStore, HistoryStore, RunLiveWorkSource, RunStatus, RunStore};
use crate::usage::UsageCacheStore;
use crate::{Engine, Result, WorkerScope};

use super::fixture::{profile, run, AttemptSequence, FixedClock, NoLiveWorkers, ProviderFixture};

#[test]
fn retries_the_same_provider_then_finalizes_all_state() {
    let temp = tempfile::tempdir().unwrap();
    let first = profile(Engine::Claude, "1", temp.path());
    let second = profile(Engine::Claude, "2", temp.path());
    let opencode = profile(Engine::Opencode, "1", temp.path());
    let providers = ProviderRegistry::new([
        Box::new(ProviderFixture {
            engine: Engine::Claude,
            profiles: vec![first.clone(), second.clone()],
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
    let no_worktree = |_run: &mut crate::runs::RunRecord| -> Result<()> { Ok(()) };
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
    let attempts = AttemptSequence::new([RunStatus::Limit, RunStatus::Done]);
    let scope = WorkerScope::all();
    let mut item = run(Engine::Claude, first.path.clone(), temp.path());
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
        continuation: None,
        claims: None,
    }
    .execute(&mut item)
    .unwrap();
    assert_eq!(result.status, RunStatus::Done);
    assert_eq!(item.attempt, 2);
    assert_eq!(item.profile, second.path);
    assert_eq!(attempts.observed().len(), 2);
    assert_eq!(attempts.observed()[0].2, attempts.observed()[1].2);
    assert_eq!(
        controls.cooldown_until(&first.path, 100.0).unwrap(),
        Some(500.0)
    );
    assert!(history.get("run").unwrap().is_some());
    let journal = events.read(Some("run"), 0).unwrap();
    assert!(journal.iter().any(|event| event.event == "failover"));
    assert_eq!(journal.last().unwrap().event, "finished");
}
