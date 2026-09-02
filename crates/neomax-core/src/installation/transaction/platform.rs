use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

use crate::Result;

#[cfg(not(any(unix, windows)))]
use crate::Error;

pub(super) trait RenameOps {
    fn rename(&self, source: &Path, target: &Path) -> io::Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct SystemRename;

impl RenameOps for SystemRename {
    fn rename(&self, source: &Path, target: &Path) -> io::Result<()> {
        system_rename(source, target)
    }
}

pub(super) fn sync_transaction_paths<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Result<()> {
    let mut parents = BTreeSet::new();
    for path in paths {
        if let Some(parent) = path.parent() {
            parents.insert(parent.to_path_buf());
        }
    }
    for parent in parents {
        sync_directory(&parent)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(super) fn apply_permissions(file: &File, permissions: fs::Permissions) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(permissions.mode()))?;
    }
    #[cfg(not(unix))]
    file.set_permissions(permissions)?;
    Ok(())
}

pub(super) fn open_regular_source(path: &Path) -> Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        Ok(OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        let _path_guard = crate::io::PathGuard::for_path(path)?;
        crate::io::reject_reparse_components(path)?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let file = options.open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || crate::io::reject_reparse_components(path).is_err()
        {
            return Err(crate::Error::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("refusing a reparse point or non-file source: {}", path.display()),
            )));
        }
        return Ok(file);
    }
    #[cfg(not(any(unix, windows)))]
    Ok(OpenOptions::new().read(true).open(path)?)
}

pub(super) fn create_symlink(source: &Path, target: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, target)?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        let _source_guard = crate::io::PathGuard::for_path(source)?;
        let _target_guard = crate::io::PathGuard::for_path(target)?;
        crate::io::reject_reparse_components(source)?;
        let metadata = fs::symlink_metadata(source)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(crate::Error::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("refusing a reparse point symlink source: {}", source.display()),
            )));
        }
        if metadata.is_dir() {
            std::os::windows::fs::symlink_dir(source, target)?;
        } else {
            std::os::windows::fs::symlink_file(source, target)?;
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (source, target);
        return Err(Error::InvalidArgument(
            "symbolic-link installation is unsupported on this platform".into(),
        ));
    }
    Ok(())
}

pub(super) fn is_cross_device(error: &io::Error) -> bool {
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::EXDEV) {
        return true;
    }
    #[cfg(windows)]
    if error.raw_os_error() == Some(17) {
        return true;
    }
    false
}

#[cfg(windows)]
pub(super) fn system_rename(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let _source_guard = crate::io::PathGuard::for_path(source)?;
    let _target_guard = crate::io::PathGuard::for_path(target)?;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let status = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if status == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
pub(super) fn system_rename(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn MoveFileExW(existing_file_name: *const u16, new_file_name: *const u16, flags: u32) -> i32;
}
