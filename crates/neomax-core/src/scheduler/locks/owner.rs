use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Duration;

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::io::{read_reader, ReadLimits};

const MAX_OWNER_BYTES: usize = 16 * 1024;
const OWNER_READ_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockOwner {
    pub runid: String,
    #[serde(default)]
    pub pid: Option<u32>,
    pub ts: i64,
}

impl LockOwner {
    pub fn new(runid: impl Into<String>, now: i64) -> Self {
        Self {
            runid: runid.into(),
            pid: Some(std::process::id()),
            ts: now,
        }
    }

    pub(super) fn is_valid(&self) -> bool {
        !self.runid.is_empty() && self.ts >= 0
    }
}

#[derive(Debug)]
pub(super) enum OwnerSnapshot {
    Missing,
    Valid(LockOwner),
    Malformed,
    Unavailable,
}

pub(super) fn read_owner(path: &Path) -> OwnerSnapshot {
    let mut file = match open_owner_file(path, false) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return OwnerSnapshot::Missing,
        Err(_) => return OwnerSnapshot::Unavailable,
    };
    if FileExt::lock_shared(&file).is_err() {
        return OwnerSnapshot::Unavailable;
    }
    let result = read_bounded(&mut file);
    let unlock = FileExt::unlock(&file);
    let Ok(bytes) = result else {
        let _ = unlock;
        return OwnerSnapshot::Unavailable;
    };
    if unlock.is_err() {
        return OwnerSnapshot::Unavailable;
    }
    match serde_json::from_slice::<LockOwner>(&bytes) {
        Ok(owner) if owner.is_valid() => OwnerSnapshot::Valid(owner),
        _ => OwnerSnapshot::Malformed,
    }
}

pub(super) fn create_owner(path: &Path, owner: &LockOwner) -> io::Result<File> {
    let _path_guard = crate::io::PathGuard::for_path(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options.open(path)?;
    #[cfg(windows)]
    validate_owner_file(&file, path)?;
    FileExt::lock_exclusive(&file)?;
    let mut bytes = serde_json::to_vec(owner).map_err(io::Error::other)?;
    bytes.push(b'\n');
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_data()
    })();
    let unlock = FileExt::unlock(&file);
    match (result, unlock) {
        (Ok(()), Ok(())) => Ok(file),
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
    }
}

pub(super) fn with_exclusive_owner<T>(
    path: &Path,
    operation: impl FnOnce(&mut File, OwnerSnapshot) -> io::Result<T>,
) -> io::Result<T> {
    let _path_guard = crate::io::PathGuard::for_path(path)?;
    let mut file = open_owner_file(path, true)?;
    FileExt::lock_exclusive(&file)?;
    let snapshot = read_locked_owner(&mut file)?;
    let result = operation(&mut file, snapshot);
    let unlock = FileExt::unlock(&file);
    match (result, unlock) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn open_owner_file(path: &Path, writable: bool) -> io::Result<File> {
    let _path_guard = crate::io::PathGuard::for_path(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    if writable {
        options.write(true);
    }
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    validate_owner_file(&file, path)?;
    Ok(file)
}

#[cfg(windows)]
fn validate_owner_file(file: &File, path: &Path) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing a reparse point or non-file lock path: {}", path.display()),
        ));
    }
    crate::io::reject_reparse_components(path)
}

fn read_locked_owner(file: &mut File) -> io::Result<OwnerSnapshot> {
    let bytes = read_bounded(file)?;
    Ok(match serde_json::from_slice::<LockOwner>(&bytes) {
        Ok(owner) if owner.is_valid() => OwnerSnapshot::Valid(owner),
        _ => OwnerSnapshot::Malformed,
    })
}

fn read_bounded(file: &mut File) -> io::Result<Vec<u8>> {
    let limits = ReadLimits::new(MAX_OWNER_BYTES, OWNER_READ_TIMEOUT).map_err(io::Error::other)?;
    read_reader(file, limits).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn rejects_oversized_owner_payloads_without_parsing_or_reclaiming_them() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("owner.lock");
        fs::write(&path, vec![b'x'; MAX_OWNER_BYTES + 1]).unwrap();

        assert!(matches!(read_owner(&path), OwnerSnapshot::Unavailable));
    }
}
