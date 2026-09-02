mod discovery;
mod orientation;
mod registry;
mod slug;
mod types;

pub use discovery::discover_repositories;
pub use orientation::{ProjectOrientation, safe_project_path};
pub use registry::ProjectRegistry;
pub use slug::project_slug;
pub use types::Project;

#[cfg(test)]
mod tests;
