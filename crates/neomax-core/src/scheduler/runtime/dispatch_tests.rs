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

#[test]
fn planner_resolves_the_opus_flag_to_opus_5_5_unless_a_model_is_explicit() {
    let mut flagged = part("flagged", Engine::Claude, &[], &[]);
    flagged.opus = true;
    let mut pinned = part("pinned", Engine::Claude, &[], &[]);
    pinned.opus = true;
    pinned.model = Some("claude-opus-5-5".into());
    let plan = plan(vec![flagged, pinned, part("plain", Engine::Claude, &[], &[])]);
    let planner = DefaultDispatchPlanner::new(PathBuf::from("/workspace"));
    let model = |id: &str| {
        planner
            .plan(plan_ref(&plan), plan.part(id).unwrap(), 1)
            .unwrap()
            .model
    };
    assert_eq!(model("flagged").as_deref(), Some("claude-opus-5-5[1m]"));
    assert_eq!(model("pinned").as_deref(), Some("claude-opus-5-5"));
    assert_eq!(model("plain"), None);
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
