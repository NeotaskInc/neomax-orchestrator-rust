use std::fs;
use std::path::Path;

use crate::Result;

use super::platform;

pub(super) fn link_read_only_state(profile: &Path, destination: &Path) -> Result<()> {
    for name in ["credentials", "sessions"] {
        let source = profile.join(name);
        if source.exists() {
            platform::link_directory(&source, &destination.join(name)).map_err(|error| {
                crate::Error::InvalidArgument(format!(
                    "Kimi plan mode could not prepare the read-only profile state for {name}: {error}. OAuth and API credential files are never copied; grant the supported Windows junction/symlink capability or use a profile with a supported credential source, then retry."
                ))
            })?;
        }
    }
    link_session_index(profile, destination)
}

fn link_session_index(profile: &Path, destination: &Path) -> Result<()> {
    let source = profile.join("session_index.jsonl");
    if !source.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(&source)?;
    if !metadata.file_type().is_file() {
        return Err(crate::Error::InvalidArgument(
            "Kimi session_index.jsonl is not a regular file; refusing to stage it for plan mode"
                .into(),
        ));
    }
    let destination = destination.join("session_index.jsonl");
    #[cfg(unix)]
    platform::link_file(&source, &destination)?;
    #[cfg(windows)]
    {
        platform::copy_file(&source, &destination)?;
        platform::set_file_permissions(&destination)?;
    }
    Ok(())
}
