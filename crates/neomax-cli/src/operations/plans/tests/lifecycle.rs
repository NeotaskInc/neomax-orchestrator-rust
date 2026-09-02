use std::sync::Arc;

use neomax_core::scheduler::persistence::PlanStatus;
use neomax_core::scheduler::runtime::{FixedClock, RuntimeConfig};
use neomax_core::scheduler::service::{AttachOptions, PersistencePort};

use super::super::lifecycle::PlanLifecycle;
use super::fixtures::{
    FixtureAdmission, FixtureRecovery, FixtureRunner, FixtureWorkspace, MemoryPersistence, spec,
};

#[test]
fn scheduler_lifecycle_exposes_start_tick_interrupt_and_status() {
    let fixture = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: fixture.path().join("worktrees"),
    });
    let runner = FixtureRunner::with_outcomes([(
        "batch-1-one".into(),
        vec![neomax_core::scheduler::runtime::WorkerOutcome::Completed {
            run_id: "batch-1-one".into(),
        }],
    )]);
    let mut lifecycle = PlanLifecycle::start(
        spec(fixture.path()),
        persistence.clone(),
        workspace,
        runner,
        FixtureAdmission,
        FixedClock::new(10),
    )
    .unwrap();
    let report = lifecycle.tick().unwrap();
    assert!(report.finished);
    let record = lifecycle.current_record().unwrap();
    assert_eq!(record.status, PlanStatus::Done);
    assert_eq!(record.plan_id, "batch-1");
    assert_eq!(lifecycle.service().coordinator().active_count(), 0);
    assert!(lifecycle.interrupt(None).is_ok());
}

#[test]
fn scheduler_interrupt_persists_and_attach_reopens_recoverable_plan() {
    let fixture = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: fixture.path().join("worktrees"),
    });
    let mut lifecycle = PlanLifecycle::start(
        spec(fixture.path()),
        persistence.clone(),
        Arc::clone(&workspace),
        FixtureRunner::default(),
        FixtureAdmission,
        FixedClock::new(10),
    )
    .unwrap();
    lifecycle
        .interrupt(Some("operator requested stop".into()))
        .unwrap();
    assert_eq!(
        persistence.load("batch-1").unwrap().status,
        PlanStatus::Interrupted
    );

    let mut attached = PlanLifecycle::attach(
        "batch-1",
        persistence,
        workspace,
        FixtureRunner::default(),
        FixtureAdmission,
        FixedClock::new(11),
        AttachOptions {
            runtime: RuntimeConfig::default(),
            recovery: &mut FixtureRecovery,
        },
    )
    .unwrap();
    let mut recovery = FixtureRecovery;
    let report = attached.recover(&mut recovery).unwrap();
    assert_eq!(report.completed.len(), 0);
    assert_eq!(
        attached.current_record().unwrap().status,
        PlanStatus::Running
    );
}
