use std::sync::{Arc, Mutex};

use crate::scheduler::persistence::PlanStatus;
use crate::scheduler::runtime::{
    DispatchRequest, FixedClock, RecoveredWorker, RuntimeConfig, WorkerOutcome,
};
use crate::scheduler::service::test_support::{
    one_part_plan, repository, FixtureAdmission, FixtureRunner, FixtureWorkspace, MemoryPersistence,
};
use crate::scheduler::service::{
    AttachOptions, PersistencePort, RecoveryPort, RecoveryStatus, RunAllService, RunAllSpec,
};

struct RecoveryFixture {
    status: RecoveryStatus,
    handle: Option<Box<dyn RecoveredWorker>>,
}

struct FixtureHandle {
    outcome: Option<WorkerOutcome>,
    cancelled: Arc<Mutex<usize>>,
}

impl RecoveredWorker for FixtureHandle {
    fn poll(&mut self) -> crate::Result<Option<WorkerOutcome>> {
        Ok(self.outcome.take())
    }

    fn cancel(&mut self) -> crate::Result<()> {
        *self.cancelled.lock().unwrap() += 1;
        Ok(())
    }
}

impl RecoveryPort for RecoveryFixture {
    fn inspect(
        &mut self,
        _request: &DispatchRequest,
        _execution: &crate::scheduler::PartExecution,
    ) -> crate::Result<RecoveryStatus> {
        Ok(self.status.clone())
    }

    fn live_handle(
        &mut self,
        _request: &DispatchRequest,
        _execution: &crate::scheduler::PartExecution,
    ) -> crate::Result<Option<Box<dyn RecoveredWorker>>> {
        Ok(self.handle.take())
    }
}

fn interrupted_service(
    persistence: Arc<MemoryPersistence>,
    workspace: Arc<FixtureWorkspace>,
    plan_id: &str,
) -> RunAllService<MemoryPersistence, FixtureWorkspace, FixtureRunner, FixtureAdmission, FixedClock>
{
    let mut service = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(workspace.root.parent().unwrap()),
            base: None,
            integration_branch: None,
            plan_id: plan_id.into(),
            runtime: RuntimeConfig::default(),
        },
        persistence.clone(),
        workspace,
        FixtureRunner::default(),
        FixtureAdmission::default(),
        FixedClock::new(10),
    )
    .unwrap();
    let mut record = persistence.load(plan_id).unwrap();
    record
        .state
        .mark_running(
            "one",
            format!("{plan_id}-one"),
            Some(format!("neomax/{plan_id}-one")),
            Some("account-1".into()),
            10.0,
        )
        .unwrap();
    record.status = PlanStatus::Interrupted;
    record.interrupted = true;
    persistence.save(&record).unwrap();
    *service.coordinator_mut().state_mut() = record.state;
    service
}

fn running_record(persistence: &MemoryPersistence, plan_id: &str, launched_at: i64) {
    let mut record = persistence.load(plan_id).unwrap();
    record
        .state
        .mark_running(
            "one",
            format!("{plan_id}-one"),
            Some(format!("neomax/{plan_id}-one")),
            Some("account-1".into()),
            launched_at as f64,
        )
        .unwrap();
    record.status = PlanStatus::Running;
    persistence.save(&record).unwrap();
}

#[test]
fn interrupted_running_parts_are_inspected_without_duplicate_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let mut service = interrupted_service(persistence.clone(), workspace, "plan-3");
    let mut recovery = RecoveryFixture {
        status: RecoveryStatus::Completed(WorkerOutcome::Completed {
            run_id: "plan-3-one".into(),
        }),
        handle: None,
    };
    let report = service.recover(&mut recovery).unwrap();
    assert_eq!(report.completed, vec!["one"]);
    assert_eq!(
        persistence.load("plan-3").unwrap().state.state("one"),
        Some(crate::scheduler::PartState::Done)
    );
    assert_eq!(persistence.load("plan-3").unwrap().status, PlanStatus::Done);
}

#[test]
fn a_running_recovery_is_reported_and_not_redispatched() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let mut service = interrupted_service(persistence.clone(), workspace, "plan-4");
    let mut recovery = RecoveryFixture {
        status: RecoveryStatus::StillRunning,
        handle: Some(Box::new(FixtureHandle {
            outcome: None,
            cancelled: Arc::new(Mutex::new(0)),
        })),
    };
    let report = service.recover(&mut recovery).unwrap();
    assert_eq!(report.waiting, vec!["one"]);
    assert_eq!(
        persistence.load("plan-4").unwrap().state.state("one"),
        Some(crate::scheduler::PartState::Running)
    );
}

#[test]
fn interrupt_with_recovery_cancels_durable_workers_after_marking_the_plan() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let mut service = interrupted_service(persistence.clone(), workspace, "plan-4-control");
    let cancelled = Arc::new(Mutex::new(0));
    let mut recovery = RecoveryFixture {
        status: RecoveryStatus::StillRunning,
        handle: Some(Box::new(FixtureHandle {
            outcome: None,
            cancelled: Arc::clone(&cancelled),
        })),
    };

    service
        .interrupt_with_recovery(&mut recovery, Some("operator stop".into()))
        .unwrap();

    assert_eq!(*cancelled.lock().unwrap(), 1);
    let record = persistence.load("plan-4-control").unwrap();
    assert_eq!(record.status, PlanStatus::Interrupted);
    assert!(record.interrupted);
    assert!(persistence
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|event| event.event == "worker_interrupted"));
}

#[test]
fn control_attach_interrupts_a_plan_owned_by_another_supervisor() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let mut supervisor = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(temp.path()),
            base: None,
            integration_branch: None,
            plan_id: "plan-control-attach".into(),
            runtime: RuntimeConfig::default(),
        },
        persistence.clone(),
        workspace.clone(),
        FixtureRunner::default(),
        FixtureAdmission::default(),
        FixedClock::new(10),
    )
    .unwrap();
    supervisor.tick().unwrap();
    assert!(persistence
        .load("plan-control-attach")
        .unwrap()
        .supervisor_lease
        .is_some());

    let mut control = RunAllService::attach_for_control(
        "plan-control-attach",
        persistence.clone(),
        workspace,
        FixtureRunner::default(),
        FixtureAdmission::default(),
        FixedClock::new(11),
        RuntimeConfig::default(),
    )
    .unwrap();
    let cancelled = Arc::new(Mutex::new(0));
    let mut recovery = RecoveryFixture {
        status: RecoveryStatus::StillRunning,
        handle: Some(Box::new(FixtureHandle {
            outcome: None,
            cancelled: Arc::clone(&cancelled),
        })),
    };

    control
        .interrupt_with_recovery(&mut recovery, Some("external stop".into()))
        .unwrap();
    assert_eq!(*cancelled.lock().unwrap(), 1);
    assert_eq!(
        persistence.load("plan-control-attach").unwrap().status,
        PlanStatus::Interrupted
    );
    assert!(persistence
        .load("plan-control-attach")
        .unwrap()
        .supervisor_lease
        .is_some());
}

#[test]
fn attach_rehydrates_live_workers_before_the_first_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let starter = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(temp.path()),
            base: None,
            integration_branch: None,
            plan_id: "plan-5".into(),
            runtime: RuntimeConfig::default(),
        },
        persistence.clone(),
        workspace.clone(),
        FixtureRunner::default(),
        FixtureAdmission::default(),
        FixedClock::new(10),
    )
    .unwrap();
    drop(starter);
    running_record(&persistence, "plan-5", 10);

    let mut recovery = RecoveryFixture {
        status: RecoveryStatus::StillRunning,
        handle: Some(Box::new(FixtureHandle {
            outcome: Some(WorkerOutcome::Completed {
                run_id: "plan-5-one".into(),
            }),
            cancelled: Arc::new(Mutex::new(0)),
        })),
    };
    let mut attached = RunAllService::attach(
        "plan-5",
        persistence.clone(),
        workspace,
        FixtureRunner::default(),
        FixtureAdmission::default(),
        FixedClock::new(11),
        AttachOptions {
            runtime: RuntimeConfig::default(),
            recovery: &mut recovery,
        },
    )
    .unwrap();
    assert_eq!(attached.coordinator().active_count(), 1);
    let report = attached.tick().unwrap();
    assert_eq!(report.completed, vec!["one"]);
    assert_eq!(attached.coordinator().active_count(), 0);
    assert_eq!(persistence.load("plan-5").unwrap().status, PlanStatus::Done);
    let events = persistence.events.lock().unwrap();
    assert!(!events
        .iter()
        .any(|event| event.event == "worker_dispatch_requested"));
}

#[test]
fn attach_reconciles_finished_workers_and_releases_their_areas_once() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let starter = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(temp.path()),
            base: None,
            integration_branch: None,
            plan_id: "plan-6".into(),
            runtime: RuntimeConfig::default(),
        },
        persistence.clone(),
        workspace.clone(),
        FixtureRunner::default(),
        FixtureAdmission::default(),
        FixedClock::new(10),
    )
    .unwrap();
    drop(starter);
    running_record(&persistence, "plan-6", 10);

    let mut recovery = RecoveryFixture {
        status: RecoveryStatus::Completed(WorkerOutcome::Completed {
            run_id: "plan-6-one".into(),
        }),
        handle: None,
    };
    let attached = RunAllService::attach(
        "plan-6",
        persistence.clone(),
        workspace,
        FixtureRunner::default(),
        FixtureAdmission::default(),
        FixedClock::new(11),
        AttachOptions {
            runtime: RuntimeConfig::default(),
            recovery: &mut recovery,
        },
    )
    .unwrap();
    assert_eq!(attached.coordinator().active_count(), 0);
    assert_eq!(
        persistence.load("plan-6").unwrap().state.state("one"),
        Some(crate::scheduler::PartState::Done)
    );
    let releases = persistence
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.event == "area_release_requested")
        .count();
    assert_eq!(releases, 1);
}
