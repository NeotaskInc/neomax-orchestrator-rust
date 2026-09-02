use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use fs2::FileExt;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Error, Result};

pub const JSON_READ_MAX_BYTES: usize = 16 * 1024 * 1024;
pub const JSON_READ_TIMEOUT: Duration = Duration::from_secs(5);

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    read_json_with_limits(
        path,
        crate::io::ReadLimits::new(JSON_READ_MAX_BYTES, JSON_READ_TIMEOUT)
            .expect("JSON read limits are valid"),
    )
}

pub fn read_json_with_limits<T: DeserializeOwned>(
    path: &Path,
    limits: crate::io::ReadLimits,
) -> Result<T> {
    let data = crate::io::read_file(&crate::io::LocalFileSource, path, limits).map_err(
        |error| match error {
            crate::io::BoundedIoError::NotFound { .. } => Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "state file not found",
            )),
            other => Error::from(other),
        },
    )?;
    serde_json::from_slice(&data).map_err(|error| Error::InvalidState {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

pub fn read_json_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    read_json(path).unwrap_or_default()
}

pub fn read_json_or_default_on_missing<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match read_json(path) {
        Ok(value) => Ok(value),
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error),
    }
}

pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut data = serde_json::to_vec_pretty(value)?;
    data.push(b'\n');
    write_bytes_atomic(path, &data)
}

pub fn write_bytes_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidArgument(format!("{} has no parent", path.display())))?;
    // Keep the validated directory chain pinned until the rename completes.
    // On Windows this prevents a checked parent from being replaced by a
    // junction while the temporary file is being persisted.
    let _parent_guard = crate::io::PathGuard::ensure_directory(parent).map_err(Error::Io)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(data)?;
    temp.as_file().sync_all()?;
    crate::io::set_private_open_path(temp.as_file(), temp.path())?;
    temp.persist(path).map_err(|error| Error::Io(error.error))?;
    sync_directory(parent)?;
    Ok(())
}

pub fn append_line(path: &Path, line: &[u8]) -> Result<()> {
    let _parent_guard = path
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    let mut file = open_private_append(path)?;
    file.write_all(line)?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

pub fn append_lines_locked(path: &Path, lock_path: &Path, lines: &[Vec<u8>]) -> Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let _path_parent_guard = path
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    let _lock_parent_guard = lock_path
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    let lock = open_private_lock(lock_path)?;
    FileExt::lock_exclusive(&lock)?;
    let result = (|| {
        let mut file = open_private_append(path)?;
        for line in lines {
            file.write_all(line)?;
            file.write_all(b"\n")?;
        }
        file.sync_data()?;
        Ok(())
    })();
    FileExt::unlock(&lock)?;
    result
}

pub fn with_exclusive_lock<T>(
    lock_path: &Path,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let _parent_guard = lock_path
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    let lock = open_private_lock(lock_path)?;
    FileExt::lock_exclusive(&lock)?;
    let result = operation();
    FileExt::unlock(&lock)?;
    result
}

pub fn update_json_locked<T, F>(path: &Path, lock_path: &Path, update: F) -> Result<T>
where
    T: DeserializeOwned + Serialize + Default + Clone,
    F: FnOnce(&mut T) -> Result<()>,
{
    let _parent_guard = lock_path
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    let lock = open_private_lock(lock_path)?;
    FileExt::lock_exclusive(&lock)?;
    let mut value = read_json_or_default(path);
    let result = update(&mut value).and_then(|()| write_json_atomic(path, &value));
    FileExt::unlock(&lock)?;
    result.map(|()| value)
}

pub fn update_json_locked_strict<T, F>(path: &Path, lock_path: &Path, update: F) -> Result<T>
where
    T: DeserializeOwned + Serialize + Default,
    F: FnOnce(&mut T) -> Result<()>,
{
    with_exclusive_lock(lock_path, || {
        let mut value = read_json_or_default_on_missing(path)?;
        update(&mut value)?;
        write_json_atomic(path, &value)?;
        Ok(value)
    })
}

pub fn update_existing_json_locked<T, F>(path: &Path, lock_path: &Path, update: F) -> Result<T>
where
    T: DeserializeOwned + Serialize,
    F: FnOnce(&mut T) -> Result<()>,
{
    let _parent_guard = lock_path
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    let lock = open_private_lock(lock_path)?;
    FileExt::lock_exclusive(&lock)?;
    let result = (|| {
        let mut value = read_json(path)?;
        update(&mut value)?;
        write_json_atomic(path, &value)?;
        Ok(value)
    })();
    FileExt::unlock(&lock)?;
    result
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn open_private_append(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
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
    validate_private_file_handle(&file, path)?;
    crate::io::set_private_path(path)?;
    Ok(file)
}

fn open_private_lock(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
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
    validate_private_file_handle(&file, path)?;
    crate::io::set_private_path(path)?;
    Ok(file)
}

#[cfg(windows)]
fn validate_private_file_handle(file: &File, path: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("refusing a reparse point or non-file path: {}", path.display()),
        ));
    }
    crate::io::reject_reparse_components(path)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use super::*;

    #[test]
    fn replaces_json_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        write_json_atomic(&path, &BTreeMap::from([("a".to_string(), 1)])).unwrap();
        write_json_atomic(&path, &BTreeMap::from([("b", 2)])).unwrap();
        let value: BTreeMap<String, i32> = read_json(&path).unwrap();
        assert_eq!(value, BTreeMap::from([("b".into(), 2)]));
    }

    #[test]
    fn atomic_write_creates_missing_parent_components() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nested").join("state.json");

        write_bytes_atomic(&path, b"state").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"state");
    }

    #[test]
    fn malformed_optional_state_degrades_to_default() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        fs::write(&path, b"{").unwrap();
        let value: BTreeMap<String, i32> = read_json_or_default(&path);
        assert!(value.is_empty());
    }

    #[test]
    fn bounded_json_reads_reject_oversized_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        fs::write(&path, br#"{"key":"value"}"#).unwrap();
        let limits = crate::io::ReadLimits::new(4, Duration::from_secs(1)).unwrap();
        let error = read_json_with_limits::<BTreeMap<String, String>>(&path, limits).unwrap_err();
        assert!(
            matches!(error, Error::Message(message) if message.contains("exceeded its 4-byte limit"))
        );
    }

    #[test]
    fn missing_strict_json_state_defaults_without_creating_a_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("missing.json");
        let value: BTreeMap<String, i32> = read_json_or_default_on_missing(&path).unwrap();
        assert!(value.is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn serializes_locked_read_modify_write() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        let lock = temp.path().join("state.lock");
        let value = update_json_locked::<BTreeMap<String, i32>, _>(&path, &lock, |state| {
            state.insert("a".into(), 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(value.get("a"), Some(&1));
    }

    #[test]
    fn preserves_every_concurrent_locked_update() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        let lock = temp.path().join("state.lock");
        std::thread::scope(|scope| {
            for index in 0..8 {
                let path = &path;
                let lock = &lock;
                scope.spawn(move || {
                    update_json_locked::<BTreeMap<String, i32>, _>(path, lock, |state| {
                        state.insert(index.to_string(), index);
                        Ok(())
                    })
                    .unwrap();
                });
            }
        });
        let value: BTreeMap<String, i32> = read_json(&path).unwrap();
        assert_eq!(value.len(), 8);
    }

    #[test]
    fn updates_existing_state_without_defaulting_missing_data() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        let lock = temp.path().join("state.lock");
        write_json_atomic(&path, &BTreeMap::from([("a", 1)])).unwrap();
        let value =
            update_existing_json_locked::<BTreeMap<String, i32>, _>(&path, &lock, |state| {
                state.insert("b".to_string(), 2);
                Ok(())
            })
            .unwrap();
        assert_eq!(value.len(), 2);

        fs::remove_file(&path).unwrap();
        assert!(
            update_existing_json_locked::<BTreeMap<String, i32>, _>(&path, &lock, |_| Ok(()))
                .is_err()
        );
    }

    #[test]
    fn strict_updates_never_replace_malformed_state() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        let lock = temp.path().join("state.lock");
        fs::write(&path, b"{").unwrap();
        assert!(
            update_json_locked_strict::<BTreeMap<String, i32>, _>(&path, &lock, |state| {
                state.insert("new".into(), 1);
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"{");
    }

    #[cfg(unix)]
    #[test]
    fn append_paths_and_locks_reject_symlinks_before_opening_them() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("outside");
        fs::write(&target, b"original\n").unwrap();
        let path = temp.path().join("state.jsonl");
        symlink(&target, &path).unwrap();
        assert!(append_line(&path, b"blocked").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original\n");

        let lock_target = temp.path().join("outside.lock");
        fs::write(&lock_target, b"lock").unwrap();
        let lock = temp.path().join("state.lock");
        symlink(&lock_target, &lock).unwrap();
        let normal_path = temp.path().join("normal.jsonl");
        assert!(append_lines_locked(&normal_path, &lock, &[b"blocked".to_vec()]).is_err());
        assert!(with_exclusive_lock(&lock, || Ok(())).is_err());
        assert_eq!(fs::read(&lock_target).unwrap(), b"lock");
    }

    #[cfg(windows)]
    #[test]
    fn append_paths_and_locks_reject_reparse_points_before_writing() {
        use std::os::windows::fs::symlink_file;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("outside");
        fs::write(&target, b"original\n").unwrap();
        let path = temp.path().join("state.jsonl");
        if symlink_file(&target, &path).is_err() {
            return;
        }

        assert!(append_line(&path, b"blocked").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original\n");

        let lock_target = temp.path().join("outside.lock");
        fs::write(&lock_target, b"lock").unwrap();
        let lock = temp.path().join("state.lock");
        if symlink_file(&lock_target, &lock).is_err() {
            return;
        }
        let normal_path = temp.path().join("normal.jsonl");
        assert!(append_lines_locked(&normal_path, &lock, &[b"blocked".to_vec()]).is_err());
        assert_eq!(fs::read(&lock_target).unwrap(), b"lock");
    }
}
