use std::path::Path;

use tempfile::{Builder, TempDir};

use crate::Result;

use super::{config, credentials, platform, profile_state};

pub struct PreparedHome {
    directory: TempDir,
}

impl PreparedHome {
    pub fn path(&self) -> &Path {
        self.directory.path()
    }
}

pub fn prepare(profile: &Path, state_dir: &Path) -> Result<PreparedHome> {
    let _state_guard = crate::io::PathGuard::ensure_directory(state_dir)?;
    let existing = config::read(profile)?;
    credentials::reject_embedded(&existing)?;
    let directory = Builder::new().prefix("kimi-plan-").tempdir_in(state_dir)?;
    platform::set_directory_permissions(directory.path())?;
    profile_state::link_read_only_state(profile, directory.path())?;
    let staged_config = config::with_read_only_tools(&existing)?;
    write_config(directory.path(), &staged_config)?;
    Ok(PreparedHome { directory })
}

fn write_config(directory: &Path, config: &str) -> Result<()> {
    let config_path = directory.join("config.toml");
    crate::atomic::write_bytes_atomic(&config_path, config.as_bytes())?;
    platform::set_file_permissions(&config_path)?;
    Ok(())
}
