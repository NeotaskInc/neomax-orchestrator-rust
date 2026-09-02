use std::collections::BTreeSet;

use super::super::super::{Plan, PlanState};
use crate::{Error, Result};

pub(super) fn validate_state(plan: &Plan, state: &PlanState) -> Result<()> {
    let plan_ids = plan.part_ids().map(str::to_owned).collect::<BTreeSet<_>>();
    let state_ids = state.states.keys().cloned().collect::<BTreeSet<_>>();
    if plan_ids != state_ids {
        return Err(Error::InvalidState {
            path: plan
                .plan_id
                .clone()
                .map(std::path::PathBuf::from)
                .unwrap_or_default(),
            message: "scheduler runtime state does not match plan parts".into(),
        });
    }
    Ok(())
}
