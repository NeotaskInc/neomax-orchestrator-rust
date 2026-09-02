use std::fs::{self, File};
use std::path::Path;

use crate::{Error, Result};

#[cfg(not(any(unix, windows)))]
mod other;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(not(any(unix, windows)))]
use self::other as platform;
#[cfg(unix)]
use self::unix as platform;
#[cfg(windows)]
use self::windows as platform;

pub fn ensure_private_directory(path: &Path) -> Result<()> {
    // Keep every validated component pinned while the ACL is applied. This
    // also creates missing components one at a time on Windows, so a junction
    // cannot be inserted between directory creation and permission updates.
    let _path_guard = crate::io::PathGuard::ensure_directory(path).map_err(Error::Io)?;
    set_private_directory(path)
}

pub fn enforce_private_path(path: &Path) -> Result<()> {
    set_private_path(path)
}

pub fn verify_private_path(path: &Path) -> Result<()> {
    reject_symlink(path)?;
    platform::verify_private_path(path)
}

pub fn set_private_open_path(file: &File, path: &Path) -> Result<()> {
    reject_symlink(path)?;
    platform::set_private_open_path(file, path)
}

pub fn set_private_path(path: &Path) -> Result<()> {
    reject_symlink(path)?;
    platform::set_private_path(path)
}

pub fn set_private_directory(path: &Path) -> Result<()> {
    reject_symlink(path)?;
    if !fs::metadata(path)?.is_dir() {
        return Err(Error::InvalidArgument(format!(
            "private directory is not a directory: {}",
            path.display()
        )));
    }
    platform::set_private_directory(path)
}

fn reject_symlink(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        crate::io::reject_reparse_components(path).map_err(Error::Io)?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Err(Error::Conflict(format!(
                "private path cannot be a symlink: {}",
                path.display()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
