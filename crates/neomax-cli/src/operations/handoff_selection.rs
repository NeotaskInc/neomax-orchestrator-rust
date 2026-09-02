#[path = "handoff_selection/live.rs"]
mod live;
#[path = "handoff_selection/policy.rs"]
mod policy;
#[path = "handoff_selection/source.rs"]
mod source;
#[path = "handoff_selection/types.rs"]
mod types;

pub(crate) use live::select_live_orchestrator;
pub(crate) use policy::select;
#[cfg(test)]
pub(crate) use policy::select_with_environment;
pub(crate) use policy::select_with_profile;
pub(crate) use types::{HandoffSelection, context_time};

#[cfg(test)]
#[path = "handoff_selection/tests/mod.rs"]
mod tests;
