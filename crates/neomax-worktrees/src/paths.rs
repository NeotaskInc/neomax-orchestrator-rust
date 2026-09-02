use std::path::{Component, Path, PathBuf};

use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::{Error, Result};

pub fn validate_task(value: &str) -> Result<()> {
    validate_component(value, "task slug")
}

pub fn validate_component(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || value.starts_with('.')
        || value.ends_with('.')
        || value.contains("..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(Error::InvalidArgument(format!(
            "{label} must use [A-Za-z0-9._-] without path traversal"
        )));
    }
    Ok(())
}

pub fn relative_repo(value: &Path) -> Result<()> {
    if value.as_os_str().is_empty() || value.is_absolute() || is_rooted_but_not_absolute(value) {
        return Err(Error::InvalidArgument(format!(
            "repository path must be relative to the project root: {}",
            value.display()
        )));
    }
    if value
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::InvalidArgument(format!(
            "repository path escapes the project root: {}",
            value.display()
        )));
    }
    Ok(())
}

pub fn repository_label(project_root: &Path, relative: &Path) -> Result<String> {
    relative_repo(relative)?;
    if relative == Path::new(".") {
        return project_root
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
            .ok_or_else(|| Error::InvalidArgument("project root has no usable name".into()));
    }
    let label = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("-");
    validate_component(&label, "repository label")?;
    Ok(label)
}

fn lexical_absolute(path: &Path, base: &Path) -> Result<PathBuf> {
    if is_rooted_but_not_absolute(path) || is_rooted_but_not_absolute(base) {
        return Err(Error::InvalidArgument(
            "path must not be a Windows partial root".into(),
        ));
    }
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let mut result = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}

pub fn ensure_descendant(root: &Path, child: &Path, label: &str) -> Result<()> {
    let root = lexical_absolute(root, Path::new("."))?;
    let child = lexical_absolute(child, Path::new("."))?;
    if child == root || !child.starts_with(&root) {
        return Err(Error::Conflict(format!(
            "{label} escapes managed root {}",
            root.display()
        )));
    }
    Ok(())
}

pub fn reject_symlink_if_present(path: &Path, label: &str) -> Result<()> {
    if path
        .symlink_metadata()
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(Error::Conflict(format!(
            "refusing to use symlink {label}: {}",
            path.display()
        )));
    }
    Ok(())
}

pub fn canonical_or_lexical(path: &Path) -> Result<PathBuf> {
    if is_rooted_but_not_absolute(path) {
        return Err(Error::InvalidArgument(format!(
            "path must not be a Windows partial root: {}",
            path.display()
        )));
    }
    path.canonicalize()
        .map_err(Error::from)
        .or_else(|_| lexical_absolute(path, Path::new(".")))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn path_helpers_reject_windows_partial_roots() {
        for path in [Path::new(r"\outside"), Path::new(r"C:outside")] {
            assert!(canonical_or_lexical(path).is_err());
            assert!(ensure_descendant(Path::new(r"C:\managed"), path, "fixture").is_err());
        }
    }
}
