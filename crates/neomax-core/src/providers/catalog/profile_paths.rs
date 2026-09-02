use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::{Error, Result};

/// Normalize a user-supplied profile directory without allowing lexical
/// traversal. Existing symlink components are resolved so an explicit link to
/// a profile root is supported without making derived paths ambiguous.
pub(super) fn normalize_explicit_path(path: PathBuf, label: &str) -> Result<PathBuf> {
    reject_traversal(&path, label)?;
    if path.as_os_str().is_empty() {
        return Err(Error::InvalidArgument(format!("{label} cannot be empty")));
    }

    let mut missing = Vec::new();
    let mut existing = path.clone();
    loop {
        match fs::symlink_metadata(&existing) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    let resolved = fs::canonicalize(&existing).map_err(|error| {
                        Error::InvalidArgument(format!(
                            "{label} contains a broken symlink: {} ({error})",
                            existing.display()
                        ))
                    })?;
                    if !resolved.is_dir() {
                        return Err(Error::InvalidArgument(format!(
                            "{label} must resolve to a directory: {}",
                            path.display()
                        )));
                    }
                    return append_missing(&resolved, &missing);
                }
                if !metadata.is_dir() {
                    return Err(Error::InvalidArgument(format!(
                        "{label} must be a directory: {}",
                        path.display()
                    )));
                }
                let resolved = fs::canonicalize(&existing).map_err(|error| {
                    Error::InvalidArgument(format!(
                        "could not resolve {label}: {} ({error})",
                        existing.display()
                    ))
                })?;
                return append_missing(&resolved, &missing);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    Error::InvalidArgument(format!("{label} has no usable directory name"))
                })?;
                missing.push(name.to_os_string());
                existing = existing
                    .parent()
                    .ok_or_else(|| {
                        Error::InvalidArgument(format!("{label} has no existing parent"))
                    })?
                    .to_path_buf();
            }
            Err(error) => {
                return Err(Error::InvalidArgument(format!(
                    "could not inspect {label}: {} ({error})",
                    existing.display()
                )));
            }
        }
    }
}

/// Build a derived account directory below its resolved root. Existing
/// account links are followed only when their targets stay below that root.
pub(super) fn derived_child(root: &Path, name: &str, label: &str) -> Result<PathBuf> {
    reject_traversal(root, label)?;
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(Error::InvalidArgument(format!(
            "invalid derived {label} name"
        )));
    }

    let resolved_root = if root.is_dir() {
        let canonical = fs::canonicalize(root).map_err(|error| {
            Error::InvalidArgument(format!(
                "could not resolve configured {label} root: {} ({error})",
                root.display()
            ))
        })?;
        if !canonical.is_dir() {
            return Err(Error::InvalidArgument(format!(
                "configured {label} root is not a directory: {}",
                root.display()
            )));
        }
        canonical
    } else {
        root.to_path_buf()
    };
    let candidate = resolved_root.join(name);
    reject_traversal(&candidate, label)?;

    match fs::symlink_metadata(&candidate) {
        Ok(metadata) => {
            let resolved = fs::canonicalize(&candidate).map_err(|error| {
                Error::InvalidArgument(format!(
                    "derived {label} path is a broken symlink: {} ({error})",
                    candidate.display()
                ))
            })?;
            if !resolved.starts_with(&resolved_root) {
                return Err(Error::InvalidArgument(format!(
                    "derived {label} path escapes its configured root: {}",
                    candidate.display()
                )));
            }
            if !metadata.is_dir() && !resolved.is_dir() {
                return Err(Error::InvalidArgument(format!(
                    "derived {label} path is not a directory: {}",
                    candidate.display()
                )));
            }
            Ok(resolved)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(candidate),
        Err(error) => Err(Error::InvalidArgument(format!(
            "could not inspect derived {label} path: {} ({error})",
            candidate.display()
        ))),
    }
}

fn append_missing(base: &Path, missing: &[std::ffi::OsString]) -> Result<PathBuf> {
    let mut resolved = base.to_path_buf();
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn reject_traversal(path: &Path, label: &str) -> Result<()> {
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(Error::InvalidArgument(format!(
            "{label} cannot contain parent-directory traversal: {}",
            path.display()
        )));
    }
    Ok(())
}
