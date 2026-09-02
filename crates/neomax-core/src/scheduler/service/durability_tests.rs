use std::sync::Arc;

use crate::scheduler::runtime::{FixedClock, RuntimeConfig, WorkerOutcome};
use crate::scheduler::service::test_support::{
    one_part_plan, repository, FixtureAdmission, FixtureRunner, FixtureWorkspace, MemoryPersistence,
};
use crate::scheduler::service::{PersistencePort, RunAllService, RunAllSpec};

#[test]
fn rate_limit_transitions_are_durable_before_the_next_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let persistence = Arc::new(MemoryPersistence::default());
    let workspace = Arc::new(FixtureWorkspace {
        root: temp.path().join("worktrees"),
    });
    let runner = FixtureRunner::with_outcomes([(
        "plan-2-one".into(),
        vec![
            WorkerOutcome::RateLimited {
                run_id: "plan-2-one".into(),
                retry_at: Some(10),
                error: Some("quota".into()),
            },
            WorkerOutcome::Completed {
                run_id: "plan-2-one".into(),
            },
        ],
    )]);
    let mut service = RunAllService::start(
        RunAllSpec {
            plan: one_part_plan(),
            repository: repository(temp.path()),
            base: None,
            integration_branch: None,
            plan_id: "plan-2".into(),
            runtime: RuntimeConfig {
                max_live: 1,
                max_stall_cycles: 2,
                max_attempts: 2,
            },
        },
        persistence.clone(),
        workspace,
        runner,
        FixtureAdmission::default(),
        FixedClock::new(10),
    )
    .unwrap();
    let first = service.tick().unwrap();
    assert_eq!(first.retried, vec!["one"]);
    assert_eq!(
        persistence.load("plan-2").unwrap().state.state("one"),
        Some(crate::scheduler::PartState::Pending)
    );
    let second = service.tick().unwrap();
    assert!(second.finished);
}
