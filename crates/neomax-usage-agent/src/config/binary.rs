use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use neomax_core::io::{is_rooted_but_not_absolute, os_str_to_utf8};

pub(super) fn resolve_required_binary(
    name: &str,
    raw: &OsStr,
    path: Option<&OsStr>,
) -> Result<PathBuf> {
    let raw_text = os_str_to_utf8(name, raw)?;
    resolve_binary(raw_text, path)
        .with_context(|| format!("{name} must resolve to an existing absolute executable"))
}

pub(super) fn resolve_binary(raw: &str, path: Option<&OsStr>) -> Result<PathBuf> {
    let candidate = Path::new(raw);
    if is_path_like_binary(raw) {
        if !candidate.is_absolute() || is_rooted_but_not_absolute(candidate) {
            bail!("binary path {raw} must be absolute")
        }
        return existing_binary_with_platform_extension(candidate);
    }
    let path = path.ok_or_else(|| anyhow::anyhow!("PATH is not set while resolving {raw}"))?;
    for directory in env::split_paths(path) {
        if directory.as_os_str().is_empty() {
            continue;
        }
        if !directory.is_absolute() || is_rooted_but_not_absolute(&directory) {
            bail!("PATH must contain only absolute directories")
        }
        let candidate = directory.join(raw);
        if let Ok(found) = existing_binary_with_platform_extension(&candidate) {
            return Ok(found);
        }
    }
    bail!("binary {raw} was not found on PATH")
}

fn is_path_like_binary(raw: &str) -> bool {
    let candidate = Path::new(raw);
    candidate.is_absolute() || raw.contains('/') || raw.contains('\\')
}

fn existing_binary_with_platform_extension(path: &Path) -> Result<PathBuf> {
    let error = match existing_binary(path) {
        Ok(found) => return Ok(found),
        Err(error) => error,
    };
    #[cfg(windows)]
    if path.extension().is_none() {
        if let Ok(found) = existing_binary(&path.with_extension("exe")) {
            return Ok(found);
        }
    }
    Err(error)
}

fn existing_binary(path: &Path) -> Result<PathBuf> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("binary {} does not exist", path.display()))?;
    if !metadata.is_file() {
        bail!("binary {} is not a file", path.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("binary {} is not executable", path.display());
        }
    }
    std::fs::canonicalize(path)
        .with_context(|| format!("could not canonicalize binary {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{is_path_like_binary, resolve_binary};
    use std::ffi::OsStr;
    #[cfg(windows)]
    use std::path::Path;

    #[test]
    fn path_like_detection_accepts_both_separator_styles() {
        assert!(is_path_like_binary("tools/neomax"));
        assert!(is_path_like_binary(r"tools\neomax"));
        assert!(!is_path_like_binary("neomax"));
    }

    #[test]
    fn path_like_binary_overrides_must_be_absolute() {
        let error = resolve_binary("tools/neomax", Some(OsStr::new("/bin"))).unwrap_err();
        assert!(error.to_string().contains("absolute"));
    }

    #[test]
    fn relative_path_entries_are_not_used_for_binary_resolution() {
        let error = resolve_binary("neomax", Some(OsStr::new("relative:/bin"))).unwrap_err();
        assert!(error.to_string().contains("absolute directories"));
    }

    #[cfg(windows)]
    #[test]
    fn partial_windows_binary_roots_are_rejected() {
        for value in [r"\tools\neomax.exe", r"C:tools\neomax.exe"] {
            let error = resolve_binary(value, None).unwrap_err();
            assert!(error.to_string().contains("absolute"), "{value}");
        }
        assert!(Path::new(r"C:\tools\neomax.exe").is_absolute());
    }
}
