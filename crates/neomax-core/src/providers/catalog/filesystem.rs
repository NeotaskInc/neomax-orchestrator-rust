use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::io::{read_file, BoundedIoError, LocalFileSource, ReadLimits};
use crate::{Error, Result};

const MAX_PROFILE_FILE_BYTES: usize = 2 * 1024 * 1024;
const PROFILE_FILE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PROFILE_CHILDREN: usize = 4096;

pub trait FileSystem: Send + Sync {
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>>;
    fn children(&self, path: &Path) -> Result<Vec<PathBuf>>;
}

pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        let limits = ReadLimits::new(MAX_PROFILE_FILE_BYTES, PROFILE_FILE_TIMEOUT)
            .expect("profile file limits are valid");
        match read_file(&LocalFileSource, path, limits) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(BoundedIoError::NotFound { .. }) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn children(&self, path: &Path) -> Result<Vec<PathBuf>> {
        match std::fs::read_dir(path) {
            Ok(entries) => {
                let mut children = Vec::new();
                for entry in entries {
                    if children.len() >= MAX_PROFILE_CHILDREN {
                        return Err(Error::Message(format!(
                            "provider profile directory {} exceeds the {}-entry limit",
                            path.display(),
                            MAX_PROFILE_CHILDREN
                        )));
                    }
                    children.push(entry.map_err(Error::Io)?.path());
                }
                Ok(children)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(Error::Io(error)),
        }
    }
}
