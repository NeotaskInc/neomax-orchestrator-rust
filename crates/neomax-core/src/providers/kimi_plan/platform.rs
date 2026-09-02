#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::io;
use std::path::Path;

pub(super) fn link_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    platform_link_directory(source, destination)
}

#[cfg(unix)]
fn platform_link_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, destination)
}

#[cfg(windows)]
fn platform_link_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    create_directory_junction(source, destination).or_else(|junction_error| {
        cleanup_link_destination(destination);
        std::os::windows::fs::symlink_dir(source, destination).map_err(|symlink_error| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "directory junction failed ({junction_error}); directory symlink fallback failed ({symlink_error})"
                ),
            )
        })
    })
}

#[cfg(windows)]
fn create_directory_junction(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;

    let source_text = crate::runtime::quote_cmd_argument(source.as_os_str())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string()))?;
    let destination_text = crate::runtime::quote_cmd_argument(destination.as_os_str())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string()))?;
    let command_line = format!("mklink /J {destination_text} {source_text}");
    let shell = crate::runtime::RuntimeEnvironment::process()
        .command_shell()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::NotFound, error.to_string()))?;
    let mut command = Command::new(shell);
    crate::providers::scrub_provider_environment(&mut command);
    let output = command
        .args(["/d", "/e:on", "/v:off", "/s", "/c"])
        .raw_arg(format!(r#""{command_line}""#))
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr);
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("mklink /J exited with {}: {}", output.status, detail.trim()),
        ))
    }
}

#[cfg(windows)]
fn cleanup_link_destination(destination: &Path) {
    if let Ok(metadata) = fs::symlink_metadata(destination) {
        if metadata.file_type().is_dir() {
            let _ = fs::remove_dir(destination);
        } else {
            let _ = fs::remove_file(destination);
        }
    }
}

#[cfg(unix)]
pub(super) fn link_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, destination)
}

#[cfg(windows)]
pub(super) fn copy_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    let _source_guard = crate::io::PathGuard::for_path(source)?;
    let _destination_guard = destination
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    let mut input = crate::io::open_regular_no_follow(source)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut output = options.open(destination)?;
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        let metadata = output.metadata()?;
        if !metadata.is_file()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Kimi session index destination is not a regular file",
            ));
        }
    }
    io::copy(&mut input, &mut output)?;
    output.sync_all()
}

pub(super) fn set_directory_permissions(path: &Path) -> crate::Result<()> {
    crate::io::set_private_directory(path)
}

pub(super) fn set_file_permissions(path: &Path) -> crate::Result<()> {
    crate::io::set_private_path(path)
}
