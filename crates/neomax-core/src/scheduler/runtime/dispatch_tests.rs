use std::path::PathBuf;

use crate::Engine;

use super::dispatch::{DefaultDispatchPlanner, DispatchPlanner};
use super::test_support::{part, plan};

#[test]
fn planner_carries_part_engine_model_dependencies_and_areas() {
    let mut worker = part("worker", Engine::Codex, &["base"], &["src/core"]);
    worker.model = Some("gpt-5.6-sol".into());
    let plan = plan(vec![part("base", Engine::Claude, &[], &[]), worker]);
    let request = DefaultDispatchPlanner::new(PathBuf::from("/workspace"))
        .plan(plan_ref(&plan), plan.part("worker").unwrap(), 2)
        .unwrap();
    assert_eq!(request.run_id, "plan-test-worker");
    assert_eq!(request.attempt, 2);
    assert_eq!(request.engine, Engine::Codex);
    assert_eq!(request.model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(request.dependencies, vec!["base"]);
    assert_eq!(request.areas, vec!["src/core"]);
    assert_eq!(request.cwd, PathBuf::from("/workspace"));
}

fn plan_ref(plan: &crate::scheduler::Plan) -> &crate::scheduler::Plan {
    plan
}

#[test]
fn planner_rejects_a_plan_without_an_id() {
    let plan =
        crate::scheduler::Plan::from_parts(vec![part("one", Engine::Claude, &[], &[])]).unwrap();
    assert!(
        DefaultDispatchPlanner::new(".")
            .plan(&plan, plan.part("one").unwrap(), 1)
            .is_err()
    );
}
