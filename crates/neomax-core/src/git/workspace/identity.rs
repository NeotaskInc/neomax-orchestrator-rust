use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::git::{invoke, output};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryIdentity {
    pub root: PathBuf,
    pub common_git_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeIdentity {
    pub root: PathBuf,
    pub common_git_dir: PathBuf,
    pub branch: String,
    pub head: String,
}

pub fn repository_identity(path: &Path) -> Result<RepositoryIdentity> {
    let root = canonical_git_path(path, "--show-toplevel")?;
    let common_git_dir = git_common_dir(&root)?;
    Ok(RepositoryIdentity {
        root,
        common_git_dir,
    })
}

pub fn worktree_identity(path: &Path) -> Result<WorktreeIdentity> {
    let root = canonical_git_path(path, "--show-toplevel")?;
    let common_git_dir = git_common_dir(&root)?;
    let branch = output(&root, ["branch", "--show-current"])?;
    if branch.is_empty() {
        return Err(Error::Conflict(format!(
            "worktree {} is detached",
            root.display()
        )));
    }
    let head = output(&root, ["rev-parse", "HEAD"])?;
    Ok(WorktreeIdentity {
        root,
        common_git_dir,
        branch,
        head,
    })
}

pub fn verify_repository(path: &Path, expected: &RepositoryIdentity) -> Result<()> {
    let actual = repository_identity(path)?;
    if actual != *expected {
        return Err(Error::Conflict(format!(
            "repository identity mismatch: expected {}, got {}",
            expected.root.display(),
            actual.root.display()
        )));
    }
    Ok(())
}

pub fn verify_worktree(
    path: &Path,
    expected_repository: &RepositoryIdentity,
    expected_branch: &str,
) -> Result<WorktreeIdentity> {
    if !path.is_dir() {
        return Err(Error::NotFound(format!("worktree {}", path.display())));
    }
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(Error::Conflict(format!(
            "refusing symlink worktree {}",
            path.display()
        )));
    }
    let actual = worktree_identity(path)?;
    let expected_root = path.canonicalize()?;
    if actual.root != expected_root
        || actual.common_git_dir != expected_repository.common_git_dir
        || actual.branch != expected_branch
    {
        return Err(Error::Conflict(format!(
            "worktree identity mismatch at {}",
            path.display()
        )));
    }
    Ok(actual)
}

fn canonical_git_path(path: &Path, argument: &str) -> Result<PathBuf> {
    if crate::io::is_rooted_but_not_absolute(path) {
        return Err(Error::InvalidArgument(
            "Git working directory must not be a Windows partial root".into(),
        ));
    }
    let result = invoke(path, [OsStr::new("rev-parse"), OsStr::new(argument)])?;
    if !result.success {
        return Err(Error::Message(result.stderr_text()));
    }
    let value = result.stdout_text();
    if value.is_empty() {
        return Err(Error::Message(format!(
            "Git returned an empty path for {}",
            path.display()
        )));
    }
    resolve_git_path(path, PathBuf::from(value), "Git path")
}

fn git_common_dir(path: &Path) -> Result<PathBuf> {
    let value = output(path, ["rev-parse", "--git-common-dir"])?;
    resolve_git_path(path, PathBuf::from(value), "Git common directory")
}

fn resolve_git_path(path: &Path, candidate: PathBuf, label: &str) -> Result<PathBuf> {
    if crate::io::is_rooted_but_not_absolute(path)
        || crate::io::is_rooted_but_not_absolute(&candidate)
    {
        return Err(Error::InvalidArgument(format!(
            "{label} must not be a Windows partial root"
        )));
    }
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        path.join(candidate)
    };
    Ok(candidate.canonicalize()?)
}

#[cfg(all(test, windows))]
mod path_tests {
    use super::*;

    #[test]
    fn git_identity_paths_reject_windows_partial_roots() {
        let base = Path::new(r"C:\fixture\repo");
        assert!(resolve_git_path(base, PathBuf::from(r"\outside"), "Git path").is_err());
        assert!(resolve_git_path(base, PathBuf::from(r"C:outside"), "Git path").is_err());
    }
}
