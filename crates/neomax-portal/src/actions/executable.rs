use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use neomax_core::installation::InstallPaths;
use neomax_core::io::is_rooted_but_not_absolute;

pub(crate) const NEOMAX_BIN_ENV: &str = "NEOMAX_BIN";

pub(crate) fn configured_neomax_binary() -> String {
    if let Some(value) = env::var_os(NEOMAX_BIN_ENV) {
        return os_value(value);
    }

    resolve_installed_neomax_binary()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "neomax".into())
}

pub(crate) fn validate_neomax_binary(value: &str) -> Result<String> {
    let path = Path::new(value);
    if value.is_empty() {
        bail!("NEOMAX_BIN must not be empty");
    }
    if value.chars().any(char::is_control) {
        bail!("NEOMAX_BIN must not contain control characters");
    }
    if !path.is_absolute() || is_rooted_but_not_absolute(path) {
        bail!("NEOMAX_BIN must be an absolute executable path");
    }

    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("NEOMAX_BIN does not exist: {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("NEOMAX_BIN must not be a symlink");
    }
    if !metadata.file_type().is_file() {
        bail!("NEOMAX_BIN must name a regular file");
    }
    if !is_executable(path, &metadata) {
        bail!("NEOMAX_BIN must name an executable file");
    }

    let resolved = path
        .canonicalize()
        .with_context(|| format!("could not resolve NEOMAX_BIN: {}", path.display()))?;
    if resolved.to_string_lossy().chars().any(char::is_control) {
        bail!("NEOMAX_BIN resolves to an unsafe path");
    }
    Ok(resolved.to_string_lossy().into_owned())
}

fn resolve_installed_neomax_binary() -> Result<PathBuf> {
    if let Ok(current) = env::current_exe() {
        if let Some(parent) = current.parent() {
            let sibling = parent.join(binary_name());
            if is_regular_executable(&sibling) {
                return Ok(sibling);
            }
        }
    }

    Ok(InstallPaths::discover()?.neomax_binary())
}

fn is_regular_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    !metadata.file_type().is_symlink()
        && metadata.file_type().is_file()
        && is_executable(path, &metadata)
}

fn is_executable(path: &Path, metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    let _ = metadata;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let _ = path;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(windows)]
    {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        let _ = metadata;
        true
    }
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "neomax.exe"
    } else {
        "neomax"
    }
}

fn os_value(value: OsString) -> String {
    value.into_string().unwrap_or_else(|_| "\0".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::fs;

    #[test]
    fn accepts_a_real_absolute_executable() {
        let executable = env::current_exe().unwrap();
        let value = validate_neomax_binary(executable.to_str().unwrap()).unwrap();
        let canonical = executable.canonicalize().unwrap();
        assert_eq!(Path::new(&value), canonical.as_path());
    }

    #[test]
    fn rejects_relative_paths() {
        let error = validate_neomax_binary("neomax").unwrap_err();
        assert!(error.to_string().contains("absolute"));
    }

    #[test]
    fn rejects_control_characters_before_touching_the_filesystem() {
        let error = validate_neomax_binary("/tmp/neomax\nlocal").unwrap_err();
        assert!(error.to_string().contains("control"));
    }

    #[test]
    fn rejects_directories() {
        let directory = tempfile::tempdir().unwrap();
        let error = validate_neomax_binary(directory.path().to_str().unwrap()).unwrap_err();
        assert!(error.to_string().contains("regular file"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_executable_files() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("neomax");
        fs::write(&path, "not executable").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let error = validate_neomax_binary(path.to_str().unwrap()).unwrap_err();
        assert!(error.to_string().contains("executable"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_overrides() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target");
        fs::write(&target, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
        let link = directory.path().join("neomax");
        symlink(&target, &link).unwrap();
        let error = validate_neomax_binary(link.to_str().unwrap()).unwrap_err();
        assert!(error.to_string().contains("symlink"));
    }

    #[cfg(unix)]
    #[test]
    fn invalid_non_unicode_override_is_marked_unsafe() {
        use std::os::unix::ffi::OsStringExt;

        assert!(
            validate_neomax_binary(&os_value(OsString::from_vec(vec![
                b'/', b't', b'm', b'p', b'/', 0x80,
            ])))
            .is_err()
        );
    }
}
