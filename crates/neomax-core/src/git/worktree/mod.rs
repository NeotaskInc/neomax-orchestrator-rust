mod artifacts;
mod inspection;
mod manager;
mod state;

pub use artifacts::{
    ArtifactCleanupMode, ArtifactCleanupReport, ManagedArtifactCleaner,
};
pub use manager::GitWorktreeManager;
pub use state::{WorktreeCleanupPolicy, WorktreeInspection, WorktreeOutcome, WorktreeTarget};

#[cfg(test)]
mod tests;
