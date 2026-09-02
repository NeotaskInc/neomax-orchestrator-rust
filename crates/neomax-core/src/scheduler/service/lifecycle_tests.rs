use std::sync::{Arc, Mutex};

use crate::scheduler::persistence::PlanStatus;
use crate::scheduler::runtime::{
    AdmissionController, AdmissionDecision, DispatchRequest, FixedClock, RuntimeConfig,
    WorkerOutcome,
};
use crate::scheduler::service::test_support::{
    one_part_plan, repository, FixtureAdmission, FixtureRunner, FixtureWorkspace, MemoryPersistence,
};
use crate::scheduler::service::{PersistencePort, RunAllService, RunAllSpec};

#[derive(Clone, Default)]
struct TrackingAdmission {
    released: Arc<Mutex<Vec<String>>>,
}

impl AdmissionController for TrackingAdmission {
    fn admit(&mut self, request: &DispatchRequest, _active: usize) -> AdmissionDecision {
        AdmissionDecision::Admitted {
            areas: request.areas.clone(),
        }
    }

    fn release(&mut self, request: &DispatchRequest) {
        self.released.lock().unwrap().push(request.part_id.clone());
    }
}

#[test]
fn start_all_persists_workspace_launch_completion_and_terminal_state() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let runner = FixtureRunner::with_outcomes([(
        "plan-1-one".into(),
        vec![WorkerOutcome::Completed {
            run_id: "plan-1-one".into(),
        }],
    )]);
    let mut service = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(temp.path()),
            base: Some("main".into()),
            integration_branch: Some("neomax/int-plan-1".into()),
            plan_id: "plan-1".into(),
            runtime: RuntimeConfig {
                max_live: 1,
                max_stall_cycles: 2,
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
    assert!(report.finished);
    let record = persistence.load("plan-1").unwrap();
    assert_eq!(record.status, PlanStatus::Done);
    assert!(record.worktree.unwrap().is_dir());
    let events = persistence.events.lock().unwrap();
    for expected in [
        "plan_created",
        "integration_workspace_requested",
        "integration_workspace_ready",
        "plan_started",
        "worker_dispatch_requested",
        "worker_launched",
        "worker_poll_requested",
        "worker_outcome",
        "plan_terminal",
    ] {
        assert!(
            events.iter().any(|event| event.event == expected),
            "{expected}"
        );
    }
}

#[test]
fn interrupt_cancels_active_parts_releases_owned_leases_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let admission = TrackingAdmission::default();
    let released = Arc::clone(&admission.released);
    let mut service = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(temp.path()),
            base: None,
            integration_branch: None,
            plan_id: "plan-interrupt".into(),
            runtime: RuntimeConfig::default(),
        },
        Arc::clone(&persistence),
        workspace,
        FixtureRunner::default(),
        admission,
        FixedClock::new(10),
    )
    .unwrap();

    service.tick().unwrap();
    assert_eq!(service.coordinator().active_count(), 1);
    assert!(persistence
        .load("plan-interrupt")
        .unwrap()
        .supervisor_lease
        .is_some());

    service
        .interrupt(Some("operator requested stop".into()))
        .unwrap();
    assert_eq!(service.coordinator().active_count(), 0);
    assert_eq!(*released.lock().unwrap(), vec!["one"]);
    let interrupted = persistence.load("plan-interrupt").unwrap();
    assert_eq!(interrupted.status, PlanStatus::Interrupted);
    assert!(interrupted.supervisor_lease.is_none());

    service.interrupt(None).unwrap();
    assert_eq!(*released.lock().unwrap(), vec!["one"]);
    assert_eq!(
        persistence
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.event == "plan_interrupted")
            .count(),
        1
    );
}

#[test]
fn detach_preserves_active_workers_and_supervisor_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let mut service = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(temp.path()),
            base: None,
            integration_branch: None,
            plan_id: "plan-detach".into(),
            runtime: RuntimeConfig::default(),
        },
        Arc::clone(&persistence),
        workspace,
        FixtureRunner::default(),
        FixtureAdmission::default(),
        FixedClock::new(10),
    )
    .unwrap();

    service.tick().unwrap();
    let before = persistence.load("plan-detach").unwrap();
    assert!(before.supervisor_lease.is_some());
    assert_eq!(service.coordinator().active_count(), 1);

    service.detach().unwrap();
    let after = persistence.load("plan-detach").unwrap();
    assert_eq!(after.status, PlanStatus::Running);
    assert!(after.supervisor_lease.is_some());
    assert_eq!(service.coordinator().active_count(), 1);
    assert!(persistence
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|event| event.event == "plan_detached"));
}
