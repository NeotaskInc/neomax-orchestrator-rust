use std::sync::Arc;
use std::time::Duration;

use crate::accounts::{AccountControlStore, AccountInventory, RotationClaimStore, SelectionPolicy};
use crate::orchestration::continuation::{ContinuationPort, ContinuationService};
use crate::providers::{Provider, ProviderRegistry};
use crate::runs::coordinator::RunCoordinator;
use crate::runs::lifecycle::RunFinalizer;
use crate::runs::{EventStore, HistoryStore, RunLiveWorkSource, RunStatus, RunStore};
use crate::usage::UsageCacheStore;
use crate::{Engine, WorkerScope};

use super::fixture::{
    FixedClock, FixtureHandoff, FixtureRotation, NoLiveWorkers, ProviderFixture,
    QuotaTransitionAttempt, profile, run,
};

fn assert_live_quota_transition(engine: Engine) {
    let temp = tempfile::tempdir().unwrap();
    let first = profile(engine, "1", temp.path());
    let second = profile(engine, "2", temp.path());
    let providers = ProviderRegistry::new(
        Engine::ALL
            .into_iter()
            .map(|provider_engine| {
                let profiles = vec![
                    profile(provider_engine, "1", temp.path()),
                    profile(provider_engine, "2", temp.path()),
                ];
                Box::new(ProviderFixture {
                    engine: provider_engine,
                    profiles,
                }) as Box<dyn Provider>
            })
            .collect::<Vec<_>>(),
    );
    let usage = UsageCacheStore::new(temp.path().join("usage"));
    usage
        .save(
            engine,
            &first.path,
            &crate::usage::ProviderUsageCache {
                five_hour: crate::usage::QuotaWindow {
                    used_percent: Some(92.0),
                    resets_at: Some(500.0),
                },
                ..Default::default()
            },
        )
        .unwrap();
    usage
        .save(
            engine,
            &second.path,
            &crate::usage::ProviderUsageCache {
                five_hour: crate::usage::QuotaWindow {
                    used_percent: Some(92.0),
                    resets_at: Some(500.0),
                },
                ..Default::default()
            },
        )
        .unwrap();
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
    let no_worktree = |_run: &mut crate::runs::RunRecord| -> crate::Result<()> { Ok(()) };
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
    let attempts = QuotaTransitionAttempt::new(&usage, first.path.clone());
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
    let mut item = run(engine, first.path.clone(), temp.path());
    item.session = Some("session-1".into());
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
    assert_eq!(result.status, RunStatus::Done);
    assert_eq!(item.engine, engine);
    let in_place = matches!(engine, Engine::Claude | Engine::Codex);
    let retains_native_session = engine == Engine::Claude;
    assert_eq!(
        item.profile,
        if in_place {
            first.path.clone()
        } else {
            second.path.clone()
        }
    );
    assert_eq!(item.workdir, temp.path().join("workspace"));
    assert_eq!(
        item.session.as_deref(),
        retains_native_session.then_some("session-1"),
        "session continuity mismatch for {engine}"
    );
    assert!(
        item.session_history
            .iter()
            .any(|entry| entry.session == "session-1")
    );
    let rotation_calls = rotation.calls.lock().unwrap().clone();
    if matches!(engine, Engine::Claude | Engine::Codex) {
        assert_eq!(
            rotation_calls,
            vec![(
                temp.path().join(format!("{engine}-1")),
                temp.path().join(format!("{engine}-2")),
            )]
        );
    } else {
        assert!(rotation_calls.is_empty());
    }
    assert_eq!(
        controls.cooldown_until(&second.path, 100.0).unwrap(),
        in_place.then_some(500.0)
    );
    assert_eq!(
        controls.cooldown_until(&first.path, 100.0).unwrap(),
        (!in_place).then_some(500.0)
    );
    assert_eq!(claims.claim_count(&second.path, 100.0), 1);
    let batons = handoff.batons.lock().unwrap();
    assert_eq!(batons.len(), 1);
    let baton = &batons[0];
    if engine == Engine::Claude {
        assert_eq!(baton.five_hour, Some(99.0));
    } else {
        assert!(baton.five_hour.is_none());
    }
    assert!(baton.seven_day.is_none());
    let journal = events.read(Some("run"), 0).unwrap();
    let strategy = if matches!(engine, Engine::Claude | Engine::Codex) {
        "in_place_auth_rotation"
    } else {
        "same_provider_handoff"
    };
    assert!(
        journal
            .iter()
            .any(|event| event.event == "failover" && event.extra["strategy"] == strategy)
    );
}

#[test]
fn live_quota_transition_rotates_auth_in_place_and_continues_the_same_run() {
    assert_live_quota_transition(Engine::Claude);
}

#[test]
fn managed_worker_quota_transition_stays_same_provider_for_every_engine() {
    for engine in Engine::ALL {
        assert_live_quota_transition(engine);
    }
}
