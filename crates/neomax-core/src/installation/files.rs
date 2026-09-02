use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{Error, Result};

pub(crate) fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub(crate) fn sha256(path: &Path) -> Result<String> {
    let limits = crate::io::ReadLimits::new(2 * 1024 * 1024 * 1024, Duration::from_secs(300))?;
    crate::io::hash_file(&crate::io::LocalFileSource, path, limits).map_err(Into::into)
}

pub(crate) fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let limit = usize::try_from(maximum.min(2 * 1024 * 1024 * 1024))
        .map_err(|_| Error::InvalidArgument("installation read limit is too large".into()))?;
    let limits = crate::io::ReadLimits::new(limit, Duration::from_secs(30))?;
    crate::io::read_file(&crate::io::LocalFileSource, path, limits).map_err(Into::into)
}

pub(crate) fn validate_relative(path: &str) -> Result<()> {
    let candidate = Path::new(path);
    if path.is_empty()
        || candidate.is_absolute()
        || windows_rooted(path)
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || path
            .split(['/', '\\'])
            .any(|component| component == "..")
    {
        return Err(Error::InvalidState {
            path: PathBuf::from(path),
            message: "installation manifest contains an unsafe relative path".into(),
        });
    }
    Ok(())
}

fn windows_rooted(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with(['\\', '/']) || bytes.get(1) == Some(&b':')
}

pub(crate) fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    // Keep both directory chains stable for the complete copy.  On Windows a
    // validation-only guard would be dropped before fs::copy opens either
    // path, leaving a junction replacement window.
    let _source_guard = crate::io::PathGuard::for_path(source)?;
    let _destination_guard = destination
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    #[cfg(windows)]
    crate::io::reject_reparse_components(source)?;
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.file_type().is_file()
        || cfg!(windows) && is_reparse_point(&metadata)
    {
        return Err(Error::InvalidArgument(format!(
            "package file is not a regular file: {}",
            source.display()
        )));
    }
    if let Ok(metadata) = fs::symlink_metadata(destination) {
        if !metadata.file_type().is_file()
            || cfg!(windows) && is_reparse_point(&metadata)
        {
            return Err(Error::InvalidArgument(format!(
                "installation destination is not a regular file: {}",
                destination.display()
            )));
        }
    }
    let mut input = crate::io::open_regular_no_follow(source)?;
    let permissions = input.metadata()?.permissions();
    let mut output = open_copy_destination(destination)?;
    io::copy(&mut input, &mut output)?;
    output.set_permissions(permissions)?;
    output.sync_all()?;
    #[cfg(windows)]
    {
        crate::io::reject_reparse_components(destination)?;
        let metadata = fs::symlink_metadata(destination)?;
        if !metadata.file_type().is_file() || is_reparse_point(&metadata) {
            return Err(Error::InvalidArgument(format!(
                "installation destination is not a regular file: {}",
                destination.display()
            )));
        }
    }
    Ok(())
}

pub(crate) fn copy_executable(source: &Path, destination: &Path) -> Result<()> {
    copy_file(source, destination)?;
    set_executable(destination)
}

pub(crate) fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(crate) fn create_alias(source: &Path, destination: &Path) -> Result<()> {
    let _destination_guard = destination
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    #[cfg(windows)]
    let _source_guard = crate::io::PathGuard::for_path(source)?;
    #[cfg(windows)]
    crate::io::reject_reparse_components(destination)?;
    #[cfg(unix)]
    {
        let _ = source;
        std::os::unix::fs::symlink("neomax", destination)?;
    }
    #[cfg(windows)]
    {
        copy_file(source, destination)?;
        set_executable(destination)?;
        crate::io::reject_reparse_components(destination)?;
        let destination_metadata = fs::symlink_metadata(destination)?;
        if !destination_metadata.file_type().is_file() || is_reparse_point(&destination_metadata) {
            return Err(Error::InvalidArgument(format!(
                "installation alias destination is not a regular file: {}",
                destination.display()
            )));
        }
    }
    Ok(())
}

fn open_copy_destination(path: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(Error::InvalidArgument(format!(
                "installation destination is not a regular file: {}",
                path.display()
            )));
        }
        crate::io::reject_reparse_components(path)?;
    }
    Ok(file)
}

pub(crate) fn remove_empty_parent(path: &Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    let Ok(_ancestor_guard) = crate::io::PathGuard::for_path(parent) else {
        return;
    };
    let Ok(metadata) = fs::symlink_metadata(parent) else {
        return;
    };
    if !metadata.file_type().is_dir() || is_reparse_point(&metadata) {
        return;
    }
    #[cfg(windows)]
    if crate::io::reject_reparse_components(parent).is_err() {
        return;
    }
    if !fs::read_dir(parent).is_ok_and(|mut entries| entries.next().is_none()) {
        return;
    }
    #[cfg(windows)]
    if let Ok(directory) = open_deletable_directory(parent) {
        let _ = mark_directory_for_deletion(directory);
    }
    #[cfg(not(windows))]
    {
        let _ = fs::remove_dir(parent);
    }
}

#[cfg(windows)]
fn open_deletable_directory(path: &Path) -> Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let mut options = fs::OpenOptions::new();
    options
        .access_mode(FILE_READ_ATTRIBUTES | DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    let directory = options.open(path)?;
    let metadata = directory.metadata()?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err(Error::InvalidArgument(format!(
            "installation parent is not a real directory: {}",
            path.display()
        )));
    }
    Ok(directory)
}

#[cfg(windows)]
fn mark_directory_for_deletion(directory: fs::File) -> Result<()> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_FLAG_DELETE,
        FILE_DISPOSITION_FLAG_ON_CLOSE, FileDispositionInfo, FileDispositionInfoEx,
        SetFileInformationByHandle,
    };

    let handle = directory.as_raw_handle();
    let mut disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_ON_CLOSE,
    };
    let extended = unsafe {
        SetFileInformationByHandle(
            handle,
            FileDispositionInfoEx,
            (&mut disposition as *mut FILE_DISPOSITION_INFO_EX).cast(),
            u32::try_from(size_of::<FILE_DISPOSITION_INFO_EX>())
                .expect("Windows disposition structure fits in u32"),
        )
    };
    if extended == 0 {
        let mut legacy = FILE_DISPOSITION_INFO { DeleteFile: true };
        let legacy_result = unsafe {
            SetFileInformationByHandle(
                handle,
                FileDispositionInfo,
                (&mut legacy as *mut FILE_DISPOSITION_INFO).cast(),
                u32::try_from(size_of::<FILE_DISPOSITION_INFO>())
                    .expect("Windows disposition structure fits in u32"),
            )
        };
        if legacy_result == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    drop(directory);
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_relative_manifest_paths() {
        for path in [
            "bin/neomax",
            "share/neomax/workflows/neomax.md",
            r"share\neomax\README.md",
            "./share/neomax/LICENSE",
        ] {
            let result = validate_relative(path);
            assert!(result.is_ok(), "{path}: {result:?}");
        }
    }

    #[test]
    fn rejects_absolute_and_parent_manifest_paths() {
        for path in [
            "",
            "/tmp/neomax",
            "share/neomax/../outside",
            r"share\neomax\..\outside",
        ] {
            assert!(validate_relative(path).is_err(), "accepted unsafe path {path}");
        }
    }

    #[test]
    fn rejects_windows_rooted_and_drive_relative_manifest_paths_on_every_host() {
        for path in [r"\rooted", r"\server\share", r"C:drive-relative", r"C:\absolute"] {
            assert!(validate_relative(path).is_err(), "accepted unsafe path {path}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_components_cannot_escape_manifest_root() {
        for path in [Path::new(r"\rooted"), Path::new(r"C:drive-relative")] {
            assert!(windows_rooted(path.to_str().unwrap()));
            assert!(validate_relative(path.to_str().unwrap()).is_err());
        }
    }
}
