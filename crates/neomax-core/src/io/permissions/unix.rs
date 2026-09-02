use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::{Error, Result};

pub(super) fn set_private_path(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

pub(super) fn set_private_open_path(file: &File, _path: &Path) -> Result<()> {
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

pub(super) fn set_private_directory(path: &Path) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(super) fn verify_private_path(path: &Path) -> Result<()> {
    let mode = fs::metadata(path)?.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(Error::Conflict(format!(
            "private path has group or world permissions (mode {:o}): {}",
            mode & 0o777,
            path.display()
        )));
    }
    Ok(())
}
