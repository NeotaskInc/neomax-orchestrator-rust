use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use super::super::runtime::{
    DispatchError, DispatchReceipt, DispatchRequest, DispatchResult, FixedClock, WorkerOutcome,
};
use super::test_support::{
    one_part_plan, repository, FixtureAdmission, FixtureWorkspace, MemoryPersistence,
};
use super::{CoordinatorWorkerRunner, PersistencePort, RunAllService, RunAllSpec, WorkerExecution};
use crate::scheduler::runtime::RuntimeConfig;
use crate::{Error, Result};

struct DeferredProviderExecution;

impl WorkerExecution for DeferredProviderExecution {
    fn dispatch(&self, request: DispatchRequest) -> Result<DispatchReceipt> {
        Ok(DispatchReceipt::new(request.run_id, 10))
    }

    fn dispatch_classified(&self, _request: DispatchRequest) -> DispatchResult<DispatchReceipt> {
        Err(DispatchError::deferred("no eligible account"))
    }

    fn poll(&self, _run_id: &str) -> Result<Option<WorkerOutcome>> {
        Ok(None)
    }

    fn cancel(&self, _run_id: &str) -> Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeProviderExecution {
    requests: Mutex<Vec<DispatchRequest>>,
    outcomes: Mutex<BTreeMap<String, VecDeque<WorkerOutcome>>>,
}

impl FakeProviderExecution {
    fn new() -> Self {
        Self::default()
    }

    fn requests(&self) -> Vec<DispatchRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl WorkerExecution for FakeProviderExecution {
    fn dispatch(&self, request: DispatchRequest) -> Result<DispatchReceipt> {
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| Error::Message("fake provider request lock poisoned".into()))?;
        let attempt = request.attempt;
        requests.push(request.clone());
        let mut outcomes = self
            .outcomes
            .lock()
            .map_err(|_| Error::Message("fake provider outcome lock poisoned".into()))?;
        outcomes.entry(request.run_id.clone()).or_insert_with(|| {
            VecDeque::from([
                WorkerOutcome::RateLimited {
                    run_id: request.run_id.clone(),
                    retry_at: None,
                    error: Some("fixture provider failover".into()),
                },
                WorkerOutcome::Completed {
                    run_id: request.run_id.clone(),
                },
            ])
        });
        assert!(attempt >= 1);
        Ok(DispatchReceipt::new(request.run_id, 10))
    }

    fn poll(&self, run_id: &str) -> Result<Option<WorkerOutcome>> {
        Ok(self
            .outcomes
            .lock()
            .map_err(|_| Error::Message("fake provider outcome lock poisoned".into()))?
            .get_mut(run_id)
            .and_then(VecDeque::pop_front))
    }

    fn cancel(&self, _run_id: &str) -> Result<()> {
        Ok(())
    }
}

#[test]
fn run_all_uses_the_runner_boundary_for_provider_failover_and_part_attempts() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let execution = Arc::new(FakeProviderExecution::new());
    let runner = CoordinatorWorkerRunner::new(Arc::clone(&execution));
    let mut service = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(temp.path()),
            base: Some("main".into()),
            integration_branch: Some("neomax/int-fixture".into()),
            plan_id: "fixture".into(),
            runtime: RuntimeConfig {
                max_live: 1,
                max_stall_cycles: 2,
                max_attempts: 2,
            },
        },
        persistence,
        workspace,
        runner,
        FixtureAdmission::default(),
        FixedClock::new(10),
    )
    .unwrap();

    let state = service.run_until_terminal(4).unwrap();
    assert!(state.finished());
    let requests = execution.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].attempt, 1);
    assert_eq!(requests[1].attempt, 2);
}

#[test]
fn persistent_runner_keeps_temporary_provider_capacity_pending() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let runner = CoordinatorWorkerRunner::new(Arc::new(DeferredProviderExecution));
    let mut service = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(temp.path()),
            base: Some("main".into()),
            integration_branch: None,
            plan_id: "deferred".into(),
            runtime: RuntimeConfig {
                max_live: 1,
                max_stall_cycles: 1,
                max_attempts: 1,
            },
        },
        persistence.clone(),
        workspace,
        runner,
        FixtureAdmission::default(),
        FixedClock::new(10),
    )
    .unwrap();

    let report = service.tick().unwrap();
    assert_eq!(report.retried, vec!["one"]);
    assert!(!report.stalled);
    assert_eq!(
        persistence.load("deferred").unwrap().state.state("one"),
        Some(crate::scheduler::PartState::Pending)
    );
    let events = persistence.events.lock().unwrap();
    assert!(events
        .iter()
        .any(|event| event.event == "worker_dispatch_deferred"));
    assert!(!events
        .iter()
        .any(|event| event.event == "worker_dispatch_failed"));
}
