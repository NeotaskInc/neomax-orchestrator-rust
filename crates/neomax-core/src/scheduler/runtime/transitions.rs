use super::super::{PartState, PlanState};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartTransition {
    Start,
    Complete,
    Fail { error: String },
    Conflict { error: String },
    Block { dependencies: Vec<String> },
    Retry { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedTransition {
    pub previous: PartState,
    pub current: PartState,
}

pub fn apply_transition(
    state: &mut PlanState,
    part_id: &str,
    transition: PartTransition,
) -> Result<AppliedTransition> {
    let previous = state
        .state(part_id)
        .ok_or_else(|| Error::NotFound(format!("scheduler part {part_id}")))?;
    let current = match transition {
        PartTransition::Start => {
            require(previous, &[PartState::Pending], part_id, "start")?;
            PartState::Running
        }
        PartTransition::Complete => {
            require(previous, &[PartState::Running], part_id, "complete")?;
            PartState::Done
        }
        PartTransition::Fail { .. } => {
            require(
                previous,
                &[PartState::Pending, PartState::Running],
                part_id,
                "fail",
            )?;
            PartState::Failed
        }
        PartTransition::Conflict { .. } => {
            require(
                previous,
                &[PartState::Pending, PartState::Running],
                part_id,
                "conflict",
            )?;
            PartState::Conflict
        }
        PartTransition::Block { .. } => {
            require(previous, &[PartState::Pending], part_id, "block")?;
            PartState::Blocked
        }
        PartTransition::Retry { .. } => {
            require(
                previous,
                &[PartState::Failed, PartState::Running],
                part_id,
                "retry",
            )?;
            PartState::Pending
        }
    };
    state.states.insert(part_id.to_string(), current);
    if current != PartState::Running {
        state.executions.remove(part_id);
    }
    Ok(AppliedTransition { previous, current })
}

fn require(
    current: PartState,
    allowed: &[PartState],
    part_id: &str,
    operation: &str,
) -> Result<()> {
    if allowed.contains(&current) {
        return Ok(());
    }
    Err(Error::Conflict(format!(
        "cannot {operation} scheduler part {part_id} from {}",
        state_name(current)
    )))
}

fn state_name(state: PartState) -> &'static str {
    match state {
        PartState::Pending => "pending",
        PartState::Running => "running",
        PartState::Done => "done",
        PartState::Failed => "failed",
        PartState::Conflict => "conflict",
        PartState::Blocked => "blocked",
        PartState::Unknown => "unknown",
    }
}
