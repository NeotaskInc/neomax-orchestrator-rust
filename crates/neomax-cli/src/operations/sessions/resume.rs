mod model;
mod normalize;
mod ownership;
mod selection;

pub(crate) use model::ResumeTarget;
pub(crate) use normalize::normalize;
pub(crate) use selection::{resolve_target, resolve_target_for_engine};
