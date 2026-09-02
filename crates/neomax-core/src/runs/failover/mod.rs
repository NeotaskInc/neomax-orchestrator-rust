mod model;
mod order;
mod planner;
mod transition;
mod types;

pub use model::{ModelResolver, NoModelOverrides};
pub use order::cross_provider_order;
pub use planner::plan_failover;
pub use transition::{
    apply_failover, apply_failover_with_resolver, CROSS_PROVIDER_NOTE, SAME_PROVIDER_NOTE,
};
pub use types::{FailoverDecision, FailoverStop, FailoverTarget};

#[cfg(test)]
mod tests;
