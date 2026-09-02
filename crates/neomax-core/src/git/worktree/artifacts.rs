use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::git::inspection::{GitCommandRunner, ProcessGitRunner};
use crate::{Error, Result};

use super::state::WorktreeTarget;

const GENERATED_DIRECTORY_NAMES: &[&str] = &[
    "node_modules",
    "target",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".parcel-cache",
    ".vite",
    "coverage",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "__pycache__",
    ".gradle",
    "dist",
    "build",
    "out",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactCleanupMode {
    DryRun,
    Apply,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactCleanupReport {
    pub found: u64,
    pub eligible: u64,
    pub removed: u64,
    pub skipped: u64,
    pub bytes_reclaimable: u64,
    pub bytes_reclaimed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedArtifactCleaner {
    managed_root: PathBuf,
}

impl ManagedArtifactCleaner {
    pub fn new(managed_root: impl Into<PathBuf>) -> Self {
        Self {
            managed_root: managed_root.into(),
        }
    }

    pub fn managed_root(&self) -> &Path {
        &self.managed_root
    }

    pub fn cleanup(
        &self,
        target: &WorktreeTarget,
        mode: ArtifactCleanupMode,
    ) -> Result<ArtifactCleanupReport> {
        self.cleanup_with_runner(target, mode, &ProcessGitRunner)
    }

    pub fn cleanup_with_runner<R: GitCommandRunner>(
        &self,
        target: &WorktreeTarget,
        mode: ArtifactCleanupMode,
        runner: &R,
    ) -> Result<ArtifactCleanupReport> {
        verify_target(runner, &self.managed_root, target)?;
        let candidates = find_candidates(&target.worktree)?;
        let mut report = ArtifactCleanupReport {
            found: candidates.len() as u64,
            ..ArtifactCleanupReport::default()
        };

        let mut eligible = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if !candidate_is_eligible(runner, &target.worktree, &candidate)? {
                report.skipped = report.skipped.saturating_add(1);
                continue;
            }
            let bytes = directory_size(&candidate);
            report.eligible = report.eligible.saturating_add(1);
            report.bytes_reclaimable = report.bytes_reclaimable.saturating_add(bytes);
            eligible.push((candidate, bytes));
        }

        if mode == ArtifactCleanupMode::Apply {
            for (candidate, bytes) in eligible {
                if !candidate_is_eligible(runner, &target.worktree, &candidate)? {
                    report.skipped = report.skipped.saturating_add(1);
                    report.eligible = report.eligible.saturating_sub(1);
                    report.bytes_reclaimable = report.bytes_reclaimable.saturating_sub(bytes);
                    continue;
                }
                remove_candidate(&candidate)?;
                report.removed = report.removed.saturating_add(1);
                report.bytes_reclaimed = report.bytes_reclaimed.saturating_add(bytes);
            }
        }

        Ok(report)
    }
}

fn verify_target<R: GitCommandRunner>(
    runner: &R,
    managed_root: &Path,
    target: &WorktreeTarget,
) -> Result<()> {
    if [managed_root, &target.repository, &target.worktree]
        .into_iter()
        .any(crate::io::is_rooted_but_not_absolute)
    {
        return Err(Error::InvalidArgument(
            "worktree cleanup paths must not be Windows partial roots".into(),
        ));
    }
    reject_symlink(managed_root, "managed worktree root")?;
    let managed_root = managed_root.canonicalize().map_err(|error| {
        Error::Conflict(format!(
            "managed worktree root {} is unavailable: {error}",
            managed_root.display()
        ))
    })?;
    reject_symlink(&target.worktree, "worktree")?;
    let worktree = target.worktree.canonicalize()?;
    let repository = target.repository.canonicalize()?;
    if worktree == repository {
        return Err(Error::Conflict(
            "refusing cleanup of the repository itself".into(),
        ));
    }
    let relative = worktree.strip_prefix(&managed_root).map_err(|_| {
        Error::Conflict(format!(
            "worktree {} is outside managed root {}",
            worktree.display(),
            managed_root.display()
        ))
    })?;
    if relative.as_os_str().is_empty() {
        return Err(Error::Conflict(
            "refusing cleanup of the managed worktree root".into(),
        ));
    }

    let repository_root = git_path(runner, &target.repository, ["rev-parse", "--show-toplevel"])?;
    if repository_root != repository {
        return Err(Error::Conflict(format!(
            "recorded repository mismatch: expected {}, got {}",
            repository.display(),
            repository_root.display()
        )));
    }
    let worktree_root = git_path(runner, &target.worktree, ["rev-parse", "--show-toplevel"])?;
    if worktree_root != worktree {
        return Err(Error::Conflict(format!(
            "recorded worktree mismatch: expected {}, got {}",
            worktree.display(),
            worktree_root.display()
        )));
    }
    let repository_common = git_common_dir(runner, &repository)?;
    let worktree_common = git_common_dir(runner, &worktree)?;
    if repository_common != worktree_common {
        return Err(Error::Conflict(format!(
            "worktree {} does not belong to recorded repository {}",
            worktree.display(),
            repository.display()
        )));
    }
    let branch = git_text(runner, &worktree, ["branch", "--show-current"])?;
    if branch != target.branch {
        return Err(Error::Conflict(format!(
            "recorded branch mismatch at {}: expected {}, got {}",
            worktree.display(),
            target.branch,
            branch
        )));
    }
    Ok(())
}

fn git_common_dir<R: GitCommandRunner>(runner: &R, path: &Path) -> Result<PathBuf> {
    let value = git_text(runner, path, ["rev-parse", "--git-common-dir"])?;
    let candidate = PathBuf::from(value);
    if crate::io::is_rooted_but_not_absolute(path)
        || crate::io::is_rooted_but_not_absolute(&candidate)
    {
        return Err(Error::InvalidArgument(
            "Git common directory must not be a Windows partial root".into(),
        ));
    }
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        path.join(candidate)
    };
    candidate.canonicalize().map_err(Error::from)
}

fn git_path<R, I, S>(runner: &R, path: &Path, args: I) -> Result<PathBuf>
where
    R: GitCommandRunner,
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let value = git_text(runner, path, args)?;
    let candidate = PathBuf::from(value);
    if crate::io::is_rooted_but_not_absolute(path)
        || crate::io::is_rooted_but_not_absolute(&candidate)
    {
        return Err(Error::InvalidArgument(
            "Git path must not be a Windows partial root".into(),
        ));
    }
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        path.join(candidate)
    };
    candidate.canonicalize().map_err(Error::from)
}

fn git_text<R, I, S>(runner: &R, path: &Path, args: I) -> Result<String>
where
    R: GitCommandRunner,
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = args
        .into_iter()
        .map(|argument| argument.as_ref().to_owned())
        .collect::<Vec<_>>();
    let output = runner.run(path, &args)?;
    if !output.success {
        return Err(Error::Message(if output.stderr.is_empty() {
            format!("Git command failed in {}", path.display())
        } else {
            output.stderr
        }));
    }
    Ok(output.stdout.trim().to_owned())
}

fn find_candidates(worktree: &Path) -> Result<Vec<PathBuf>> {
    let mut directories = vec![worktree.to_path_buf()];
    let mut candidates = Vec::new();
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            if name == OsStr::new(".git") {
                continue;
            }
            if is_generated_name(&name) {
                candidates.push(path);
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            directories.push(path);
        }
    }
    Ok(candidates)
}

fn is_generated_name(name: &OsStr) -> bool {
    GENERATED_DIRECTORY_NAMES
        .iter()
        .any(|candidate| OsStr::new(candidate) == name)
}

fn candidate_is_eligible<R: GitCommandRunner>(
    runner: &R,
    worktree: &Path,
    candidate: &Path,
) -> Result<bool> {
    let metadata = match fs::symlink_metadata(candidate) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(Error::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(false);
    }
    let relative = candidate
        .strip_prefix(worktree)
        .map_err(|_| Error::Conflict("candidate escaped worktree".into()))?;
    let Some(relative) = relative.to_str() else {
        return Ok(false);
    };
    let ignored = runner.run(
        worktree,
        &[
            "check-ignore".into(),
            "-q".into(),
            "--".into(),
            relative.to_owned(),
        ],
    )?;
    if !ignored.success {
        if ignored.stderr.is_empty() {
            return Ok(false);
        }
        return Err(Error::Message(ignored.stderr));
    }
    let tracked = runner.run(
        worktree,
        &[
            "ls-files".into(),
            "-z".into(),
            "--".into(),
            relative.to_owned(),
        ],
    )?;
    if !tracked.success {
        return Err(Error::Message(if tracked.stderr.is_empty() {
            format!("unable to inspect tracked files under {relative}")
        } else {
            tracked.stderr
        }));
    }
    Ok(tracked.stdout.is_empty())
}

fn directory_size(path: &Path) -> u64 {
    let mut directories = vec![path.to_path_buf()];
    let mut total = 0u64;
    while let Some(directory) = directories.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                directories.push(path);
            } else {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

fn remove_candidate(path: &Path) -> Result<()> {
    reject_symlink(path, "generated artifact")?;
    fs::remove_dir_all(path).map_err(Error::from)
}

fn reject_symlink(path: &Path, label: &str) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(Error::Conflict(format!(
                "refusing symlink {label} {}",
                path.display()
            )));
        }
    }
    Ok(())
}
