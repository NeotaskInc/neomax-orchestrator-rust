use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};

use crate::{Engine, Error, Result};

use super::admission::{AdmissionController, AdmissionDecision};
use super::clock::FixedClock;
use super::coordinator::{RuntimeConfig, RuntimeCoordinator};
use super::dispatch::{
    DefaultDispatchPlanner, DispatchError, DispatchReceipt, DispatchRequest, WorkerOutcome,
    WorkerRunner,
};
use super::test_support::{part, pending_state, plan};

#[derive(Default)]
struct FakeRunner {
    outcomes: BTreeMap<String, VecDeque<WorkerOutcome>>,
    dispatched: Vec<String>,
    cancelled: Arc<Mutex<Vec<String>>>,
    cancel_failures: usize,
}

impl FakeRunner {
    fn with_outcomes(outcomes: impl IntoIterator<Item = (String, Vec<WorkerOutcome>)>) -> Self {
        Self {
            outcomes: outcomes
                .into_iter()
                .map(|(run_id, values)| (run_id, values.into()))
                .collect(),
            dispatched: Vec::new(),
            cancelled: Arc::new(Mutex::new(Vec::new())),
            cancel_failures: 0,
        }
    }
}

impl WorkerRunner for FakeRunner {
    fn dispatch(&mut self, request: DispatchRequest) -> Result<DispatchReceipt> {
        self.dispatched.push(request.part_id);
        Ok(DispatchReceipt::new(request.run_id, 10))
    }

    fn poll(&mut self, run_id: &str) -> Result<Option<WorkerOutcome>> {
        Ok(self.outcomes.get_mut(run_id).and_then(VecDeque::pop_front))
    }

    fn cancel(&mut self, run_id: &str) -> Result<()> {
        if self.cancel_failures != 0 {
            self.cancel_failures -= 1;
            return Err(Error::Message("fixture cancellation failed".into()));
        }
        self.cancelled.lock().unwrap().push(run_id.to_owned());
        Ok(())
    }
}

struct FakeAdmission {
    maximum: usize,
    busy_parts: BTreeSet<String>,
    released: Arc<Mutex<Vec<String>>>,
}

impl FakeAdmission {
    fn new(maximum: usize) -> Self {
        Self {
            maximum,
            busy_parts: BTreeSet::new(),
            released: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl AdmissionController for FakeAdmission {
    fn admit(&mut self, request: &DispatchRequest, active: usize) -> AdmissionDecision {
        if active >= self.maximum {
            return AdmissionDecision::CapacityExhausted {
                active,
                maximum: self.maximum,
            };
        }
        if self.busy_parts.remove(&request.part_id) {
            AdmissionDecision::AreaBusy {
                areas: request.areas.clone(),
            }
        } else {
            AdmissionDecision::Admitted {
                areas: request.areas.clone(),
            }
        }
    }

    fn release(&mut self, request: &DispatchRequest) {
        self.released.lock().unwrap().push(request.part_id.clone());
    }
}

#[test]
fn coordinator_dispatches_only_after_dependencies_succeed() {
    let plan = plan(vec![
        part("base", Engine::Claude, &[], &[]),
        part("child", Engine::Codex, &["base"], &[]),
    ]);
    let runner = FakeRunner::with_outcomes([
        (
            "plan-test-base".into(),
            vec![WorkerOutcome::Completed {
                run_id: "plan-test-base".into(),
            }],
        ),
        (
            "plan-test-child".into(),
            vec![WorkerOutcome::Completed {
                run_id: "plan-test-child".into(),
            }],
        ),
    ]);
    let mut coordinator = RuntimeCoordinator::new(
        plan.clone(),
        pending_state(&plan),
        runner,
        FakeAdmission::new(2),
        FixedClock::new(10),
        DefaultDispatchPlanner::new("."),
        RuntimeConfig {
            max_live: 2,
            max_stall_cycles: 3,
            max_attempts: 2,
        },
    )
    .unwrap();
    let first = coordinator.tick().unwrap();
    assert_eq!(first.launched, vec!["base"]);
    assert_eq!(first.completed, vec!["base"]);
    assert_eq!(
        coordinator.state().state("child"),
        Some(crate::scheduler::PartState::Pending)
    );
    let second = coordinator.tick().unwrap();
    assert_eq!(second.launched, vec!["child"]);
    assert!(second.finished);
    assert!(coordinator.state().finished());
}

#[test]
fn rate_limit_requeues_a_part_until_the_attempt_budget_is_reached() {
    let plan = plan(vec![part("one", Engine::Claude, &[], &[])]);
    let runner = FakeRunner::with_outcomes([(
        "plan-test-one".into(),
        vec![
            WorkerOutcome::RateLimited {
                run_id: "plan-test-one".into(),
                retry_at: Some(10),
                error: Some("quota".into()),
            },
            WorkerOutcome::Completed {
                run_id: "plan-test-one".into(),
            },
        ],
    )]);
    let mut coordinator = RuntimeCoordinator::new(
        plan.clone(),
        pending_state(&plan),
        runner,
        FakeAdmission::new(1),
        FixedClock::new(10),
        DefaultDispatchPlanner::new("."),
        RuntimeConfig {
            max_live: 1,
            max_stall_cycles: 3,
            max_attempts: 2,
        },
    )
    .unwrap();
    let first = coordinator.tick().unwrap();
    assert_eq!(first.retried, vec!["one"]);
    assert_eq!(coordinator.attempt("one"), 1);
    let second = coordinator.tick().unwrap();
    assert_eq!(second.completed, vec!["one"]);
    assert!(coordinator.state().finished());
}

#[derive(Default)]
struct DeferredRunner;

impl WorkerRunner for DeferredRunner {
    fn dispatch(&mut self, request: DispatchRequest) -> Result<DispatchReceipt> {
        Ok(DispatchReceipt::new(request.run_id, 10))
    }

    fn dispatch_classified(
        &mut self,
        _request: DispatchRequest,
    ) -> super::dispatch::DispatchResult<DispatchReceipt> {
        Err(DispatchError::deferred("no eligible account"))
    }

    fn poll(&mut self, _run_id: &str) -> Result<Option<WorkerOutcome>> {
        Ok(None)
    }
}

#[test]
fn temporary_capacity_keeps_pending_parts_deferred_without_stalling() {
    let plan = plan(vec![part("one", Engine::Claude, &[], &[])]);
    let mut coordinator = RuntimeCoordinator::new(
        plan.clone(),
        pending_state(&plan),
        DeferredRunner,
        FakeAdmission::new(1),
        FixedClock::new(10),
        DefaultDispatchPlanner::new("."),
        RuntimeConfig {
            max_live: 1,
            max_stall_cycles: 1,
            max_attempts: 1,
        },
    )
    .unwrap();
    let report = coordinator.tick().unwrap();
    assert_eq!(report.retried, vec!["one"]);
    assert!(!report.stalled);
    assert_eq!(
        coordinator.state().state("one"),
        Some(crate::scheduler::PartState::Pending)
    );
}

#[test]
fn mismatched_outcome_run_ids_fail_closed() {
    let plan = plan(vec![part("one", Engine::Claude, &[], &[])]);
    let runner = FakeRunner::with_outcomes([(
        "plan-test-one".into(),
        vec![WorkerOutcome::Completed {
            run_id: "other".into(),
        }],
    )]);
    let mut coordinator = RuntimeCoordinator::new(
        plan.clone(),
        pending_state(&plan),
        runner,
        FakeAdmission::new(1),
        FixedClock::new(10),
        DefaultDispatchPlanner::new("."),
        RuntimeConfig::default(),
    )
    .unwrap();
    let error = coordinator.tick().unwrap_err();
    assert!(matches!(error, Error::Conflict(_)));
}

#[test]
fn cancelling_active_parts_releases_only_their_admission_leases() {
    let plan = plan(vec![
        part("one", Engine::Claude, &[], &["src"]),
        part("two", Engine::Codex, &[], &["docs"]),
    ]);
    let cancelled = Arc::new(Mutex::new(Vec::new()));
    let released = Arc::new(Mutex::new(Vec::new()));
    let runner = FakeRunner {
        cancelled: Arc::clone(&cancelled),
        ..FakeRunner::default()
    };
    let admission = FakeAdmission {
        released: Arc::clone(&released),
        ..FakeAdmission::new(1)
    };
    let mut coordinator = RuntimeCoordinator::new(
        plan.clone(),
        pending_state(&plan),
        runner,
        admission,
        FixedClock::new(10),
        DefaultDispatchPlanner::new("."),
        RuntimeConfig::default(),
    )
    .unwrap();
    coordinator.tick().unwrap();
    assert_eq!(coordinator.active_count(), 1);

    let cancelled_parts = coordinator.cancel_active().unwrap();

    assert_eq!(cancelled_parts, vec!["one"]);
    assert_eq!(coordinator.active_count(), 0);
    assert_eq!(*released.lock().unwrap(), vec!["one"]);
    assert_eq!(*cancelled.lock().unwrap(), vec!["plan-test-one"]);
}

#[test]
fn failed_cancellation_keeps_the_part_and_its_leases_for_retry() {
    let plan = plan(vec![part("one", Engine::Claude, &[], &["src"])]);
    let released = Arc::new(Mutex::new(Vec::new()));
    let admission = FakeAdmission {
        released: Arc::clone(&released),
        ..FakeAdmission::new(1)
    };
    let runner = FakeRunner {
        cancel_failures: 1,
        ..FakeRunner::default()
    };
    let mut coordinator = RuntimeCoordinator::new(
        plan.clone(),
        pending_state(&plan),
        runner,
        admission,
        FixedClock::new(10),
        DefaultDispatchPlanner::new("."),
        RuntimeConfig::default(),
    )
    .unwrap();
    coordinator.tick().unwrap();

    assert!(coordinator.cancel_active().is_err());
    assert_eq!(coordinator.active_count(), 1);
    assert!(released.lock().unwrap().is_empty());
    assert_eq!(coordinator.cancel_active().unwrap(), vec!["one"]);
    assert_eq!(coordinator.active_count(), 0);
    assert_eq!(*released.lock().unwrap(), vec!["one"]);
}
