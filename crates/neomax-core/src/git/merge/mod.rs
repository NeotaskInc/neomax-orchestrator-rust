mod integrator;
mod policy;
mod resolver;
mod union;

pub use integrator::{GitPartIntegrator, IntegrationOutcome, PartIntegrator};
pub use policy::is_union_safe;
pub use resolver::{resolve_safe_conflicts, ConflictResolution};
pub use union::union_resolve;

#[cfg(test)]
mod tests;
