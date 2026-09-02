use serde_json::json;

use crate::WorkerScope;
use crate::scheduler::Plan;

use super::super::record::PlanRecord;

pub fn plan(id: &str) -> Plan {
    Plan::from_value(
        json!({
            "plan": id,
            "repo": "/workspace/repository",
            "base": "main",
            "integration_branch": format!("neomax/int-{id}"),
            "parts": [
                {"id": "first", "prompt": "first"},
                {"id": "second", "prompt": "second", "depends_on": ["first"]}
            ]
        }),
        &WorkerScope::all(),
    )
    .unwrap()
}

pub fn record(id: &str) -> PlanRecord {
    PlanRecord::new(
        id,
        plan(id),
        Some(format!("/workspace/worktrees/integ-{id}").into()),
        100,
    )
    .unwrap()
}
