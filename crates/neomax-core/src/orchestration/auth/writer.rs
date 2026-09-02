use std::fs;
#[cfg(unix)]
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::io::{BoundedIoError, LocalFileSource, read_file};
use crate::{Error, Result};

use super::limits::credential_read_limits;
use super::permissions::set_private_open_path;

pub trait CredentialWriter: Send + Sync {
    fn read_optional(&self, path: &Path) -> Result<Option<Vec<u8>>>;
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()>;
    fn remove(&self, path: &Path) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FsCredentialWriter;

impl CredentialWriter for FsCredentialWriter {
    fn read_optional(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        match read_file(&LocalFileSource, path, credential_read_limits()) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(BoundedIoError::NotFound { .. }) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| Error::InvalidArgument(format!("{} has no parent", path.display())))?;
        let _parent_guard = crate::io::PathGuard::ensure_directory(parent)?;
        crate::io::set_private_directory(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        set_private_open_path(temporary.as_file(), temporary.path())?;
        temporary
            .persist(path)
            .map_err(|error| Error::Io(error.error))?;
        sync_directory(parent)
    }

    fn remove(&self, path: &Path) -> Result<()> {
        let _parent_guard = crate::io::PathGuard::for_path(path)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(crate) fn lock_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".lock");
    PathBuf::from(value)
}
