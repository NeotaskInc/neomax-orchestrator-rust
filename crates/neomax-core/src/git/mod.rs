mod command;
pub mod inspection;
pub mod merge;
pub mod pull_request;
pub mod workspace;
pub mod worktree;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::Result;

pub use command::{
    invoke, invoke_with_config, GitOutput, GitProcessConfig, DEFAULT_COMMAND_TIMEOUT,
    MAX_COMMAND_OUTPUT_BYTES,
};
pub use inspection::{GitInspection, GitInspectionRequest, GitInspector};
pub use merge::{GitPartIntegrator, IntegrationOutcome, PartIntegrator};
pub use worktree::{
    ArtifactCleanupMode, ArtifactCleanupReport, GitWorktreeManager, ManagedArtifactCleaner,
    WorktreeCleanupPolicy, WorktreeInspection, WorktreeOutcome, WorktreeTarget,
};

pub fn output(cwd: &Path, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Result<String> {
    command::checked_text(cwd, args)
}

pub fn repository_root(cwd: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(output(
        cwd,
        ["rev-parse", "--show-toplevel"],
    )?))
}

pub fn current_branch(cwd: &Path) -> Result<String> {
    output(cwd, ["branch", "--show-current"])
}
