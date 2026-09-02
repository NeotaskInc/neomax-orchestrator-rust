mod hooks;
mod manifest;
mod staging;
mod support;
mod targets;
mod uninstall;

pub(crate) use manifest::WorkflowManifest;
pub use staging::ensure_profile_workflows;
#[cfg(test)]
pub(crate) use staging::ensure_profile_workflows_at;
pub(crate) use staging::stage;
pub(crate) use uninstall::{preflight_uninstall, remove_manifest, remove_owned_files};
