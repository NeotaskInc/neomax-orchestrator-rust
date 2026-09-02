use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::git::invoke;
use crate::io::{read_file, LocalFileSource, ReadLimits};
use crate::{Error, Result};

use super::{is_union_safe, union_resolve};

const MAX_CONFLICT_FILE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictResolution {
    pub resolved: Vec<PathBuf>,
    pub remaining: Vec<PathBuf>,
}

pub fn resolve_safe_conflicts(worktree: &Path) -> Result<ConflictResolution> {
    let conflicts = conflicted_paths(worktree)?;
    let mut resolution = ConflictResolution {
        resolved: Vec::new(),
        remaining: Vec::new(),
    };
    for path in conflicts {
        if !safe_relative_git_path(&path) || !is_union_safe(&path) {
            resolution.remaining.push(path);
            continue;
        }
        let full_path = worktree.join(&path);
        let bytes = match read_file(
            &LocalFileSource,
            &full_path,
            ReadLimits::new(MAX_CONFLICT_FILE_BYTES, Duration::from_secs(30))?,
        ) {
            Ok(bytes) => bytes,
            Err(_) => {
                resolution.remaining.push(path);
                continue;
            }
        };
        let raw = match String::from_utf8(bytes) {
            Ok(raw) => raw,
            Err(_) => {
                resolution.remaining.push(path);
                continue;
            }
        };
        let Some(merged) = union_resolve(&raw) else {
            resolution.remaining.push(path);
            continue;
        };
        fs::write(&full_path, merged.as_bytes())?;
        let added = invoke(
            worktree,
            [OsStr::new("add"), OsStr::new("--"), path.as_os_str()],
        )?;
        if !added.success {
            return Err(Error::Message(added.stderr_text()));
        }
        resolution.resolved.push(path);
    }
    Ok(resolution)
}

fn safe_relative_git_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !crate::io::is_rooted_but_not_absolute(path)
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn conflicted_paths(worktree: &Path) -> Result<Vec<PathBuf>> {
    let output = invoke(worktree, ["diff", "--name-only", "--diff-filter=U", "-z"])?;
    if !output.success {
        return Err(Error::Message(output.stderr_text()));
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .map(path_from_git_bytes)
        .collect()
}

#[cfg(unix)]
fn path_from_git_bytes(value: &[u8]) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Ok(PathBuf::from(std::ffi::OsString::from_vec(value.to_vec())))
}

#[cfg(test)]
mod tests {
    use super::safe_relative_git_path;
    use std::path::Path;

    #[test]
    fn conflict_paths_are_bounded_to_relative_descendants() {
        assert!(safe_relative_git_path(Path::new("docs/CHANGELOG.md")));
        assert!(!safe_relative_git_path(Path::new("../CHANGELOG.md")));
        assert!(!safe_relative_git_path(Path::new("./CHANGELOG.md")));
    }

    #[cfg(windows)]
    #[test]
    fn conflict_paths_reject_windows_partial_roots() {
        assert!(!safe_relative_git_path(Path::new(r"\outside\CHANGELOG.md")));
        assert!(!safe_relative_git_path(Path::new(r"C:outside\CHANGELOG.md")));
    }
}

#[cfg(not(unix))]
fn path_from_git_bytes(value: &[u8]) -> Result<PathBuf> {
    String::from_utf8(value.to_vec())
        .map(PathBuf::from)
        .map_err(|_| Error::Message("Git returned a non-Unicode conflict path".into()))
}
