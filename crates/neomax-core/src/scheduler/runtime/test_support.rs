use crate::Engine;
use crate::scheduler::{Part, Plan, PlanState};

pub fn part(id: &str, engine: Engine, depends_on: &[&str], areas: &[&str]) -> Part {
    Part {
        id: id.into(),
        prompt: format!("work for {id}"),
        engine,
        model: None,
        area: areas.iter().map(|value| (*value).into()).collect(),
        depends_on: depends_on.iter().map(|value| (*value).into()).collect(),
        effort: None,
        ultra: false,
        opus: false,
        codex_model: None,
        kimi_model: None,
        order: 0,
        extra: Default::default(),
    }
}

pub fn plan(parts: Vec<Part>) -> Plan {
    let mut plan = Plan::from_parts(parts).unwrap();
    plan.plan_id = Some("plan-test".into());
    plan
}

pub fn pending_state(plan: &Plan) -> PlanState {
    PlanState::pending(plan)
}
