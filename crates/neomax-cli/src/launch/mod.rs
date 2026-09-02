mod detached;
mod entry;
mod environment;
mod execute;
mod handshake;
mod identity;
mod model_validation;
mod models;
mod options;
mod parser;
mod plan;
mod render;
mod resume;
mod rotation;
mod scope;
mod types;
mod validation;

#[cfg(test)]
mod tests;

pub(crate) use detached::write_startup_error;
pub(crate) use execute::execute_record_with_runtime;
pub(crate) use identity::invocation_name;
pub(crate) use plan::build as build_plan;
pub(crate) use rotation::{RotationReport, rotate, rotate_model_free};
pub(crate) use scope::{effective as effective_worker_scope, for_launcher as worker_scope};
pub(crate) use types::{EnvironmentPlan, LaunchOptions, LaunchPlan};

pub(crate) use entry::run;
