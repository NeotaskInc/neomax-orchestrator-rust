use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};
use neomax_core::io::{
    FileSource, LocalFileSource, LocalProcessRunner, ProcessOutput, ProcessRequest, ProcessRunner,
    ReadLimits, is_rooted_but_not_absolute, read_file_range,
};
use neomax_core::providers::scrub_provider_process_request;

use neomax_core::runs::RunRecord;

use crate::model::RunDiff;

use super::{FilesystemPortalSource, runs::load_record};

const MAX_LOG_BYTES: usize = 400_000;
const MAX_DIFF_BYTES: usize = 1_000_000;
const GIT_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) fn run_diff(source: &FilesystemPortalSource, id: &str) -> Result<RunDiff> {
    validate_run_id(id)?;
    let run = load_record(&source.paths.runs, id)?;
    let mut result = RunDiff {
        id: run.id.clone(),
        status: run.status.as_str().into(),
        worktree: run.worktree.clone(),
        files: run.files_touched.clone(),
        ..RunDiff::default()
    };
    let Some(worktree) = run.worktree.as_deref() else {
        result.error = Some("run has no worktree".into());
        return Ok(result);
    };
    let worktree = match validate_worktree(worktree, &source.paths.worktrees) {
        Ok(worktree) => worktree,
        Err(error) => {
            crate::security::log_internal("rejected run worktree", &error);
            result.worktree = None;
            result.error = Some("run worktree is unavailable".into());
            return Ok(result);
        }
    };
    let output = bounded_git_diff(&worktree);
    match output {
        Ok(output) if output.success => {
            result.patch = tail_bytes(&output.stdout, MAX_DIFF_BYTES);
        }
        Ok(output) => {
            neomax_portal_diagnostic("git diff failed", &output.stderr);
            result.error = Some("git diff unavailable".into());
        }
        Err(error) => {
            crate::security::log_internal("git diff failed", &error);
            result.error = Some("git diff unavailable".into());
        }
    }
    Ok(result)
}

fn neomax_portal_diagnostic(context: &str, bytes: &[u8]) {
    let error = String::from_utf8_lossy(bytes);
    crate::security::log_internal(context, &error);
}

fn bounded_git_diff(worktree: &Path) -> Result<ProcessOutput> {
    let request = ProcessRequest::new("git")
        .args(["diff", "--no-ext-diff", "--binary", "--"])
        .cwd(worktree)
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .timeout(GIT_TIMEOUT)
        .stdout_limit(MAX_DIFF_BYTES)
        .stderr_limit(64 * 1024);
    let request = scrub_provider_process_request(request);
    let output = LocalProcessRunner::default().capture(&request)?;
    if output.timed_out {
        bail!("git diff exceeded {} seconds", GIT_TIMEOUT.as_secs());
    }
    if output.stdout_truncated || output.stderr_truncated {
        bail!("git diff output exceeded its local limit");
    }
    Ok(output)
}

pub(crate) fn run_log(source: &FilesystemPortalSource, id: &str, limit: usize) -> Result<String> {
    validate_run_id(id)?;
    let run = load_record(&source.paths.runs, id)?;
    let path = log_path(source, &run);
    let Some(path) = path else {
        return Ok(String::new());
    };
    read_tail_file(&path, limit.clamp(1, MAX_LOG_BYTES))
}

pub fn read_run_log(path: &Path, limit: usize) -> Result<String> {
    read_tail_file(path, limit.clamp(1, MAX_LOG_BYTES))
}

fn read_tail_file(path: &Path, limit: usize) -> Result<String> {
    let length = LocalFileSource.metadata(path)?.len;
    let tail_length = limit.min(usize::try_from(length).unwrap_or(limit));
    if tail_length == 0 {
        return Ok(String::new());
    }
    let start = length.saturating_sub(tail_length as u64);
    let bytes = read_file_range(
        &LocalFileSource,
        path,
        start,
        tail_length,
        ReadLimits::new(limit.max(1), Duration::from_secs(2))?,
    )?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn log_path(source: &FilesystemPortalSource, run: &RunRecord) -> Option<PathBuf> {
    let candidates = [
        run.log.clone(),
        Some(source.paths.logs.join(format!("{}.log", run.id))),
        Some(source.paths.history_logs.join(format!("{}.log", run.id))),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|path| validate_regular_state_file(path, &source.paths.state).is_ok())
}

fn validate_regular_state_file(path: &Path, root: &Path) -> Result<()> {
    if !path.is_absolute() || !root.is_absolute() {
        bail!("state artifact paths must be absolute")
    }
    let root_metadata = fs::symlink_metadata(root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        bail!("state root is unavailable")
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| anyhow::anyhow!("state artifact is outside the state root"))?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("state artifact is outside the state root")
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            bail!("state artifact contains a symlink")
        }
    }
    let canonical_root = root.canonicalize()?;
    let canonical_path = path.canonicalize()?;
    if !canonical_path.starts_with(&canonical_root) || !fs::metadata(&canonical_path)?.is_file() {
        bail!("state artifact is outside the state root")
    }
    Ok(())
}

fn validate_worktree(worktree: &Path, root: &Path) -> Result<PathBuf> {
    if !worktree.is_absolute() || is_rooted_but_not_absolute(worktree) {
        bail!("run worktree must be absolute")
    }
    if is_rooted_but_not_absolute(root) {
        bail!("managed worktree root must not be partially rooted")
    }
    let root = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()?.join(root)
    };
    let root_metadata = fs::symlink_metadata(&root)?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        bail!("managed worktree root is unavailable")
    }
    if worktree
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("run worktree is outside the managed root")
    }
    let canonical_root = root.canonicalize()?;
    let canonical_worktree = worktree.canonicalize()?;
    if canonical_worktree == canonical_root
        || !canonical_worktree.starts_with(&canonical_root)
        || !fs::metadata(&canonical_worktree)?.is_dir()
    {
        bail!("run worktree is outside the managed root")
    }
    if relative_path_contains_symlink(worktree, &root, &canonical_root, &canonical_worktree)? {
        bail!("run worktree contains a symlink")
    }
    Ok(canonical_worktree)
}

fn relative_path_contains_symlink(
    worktree: &Path,
    root: &Path,
    canonical_root: &Path,
    canonical_worktree: &Path,
) -> Result<bool> {
    let relative = worktree
        .strip_prefix(root)
        .or_else(|_| worktree.strip_prefix(canonical_root))
        .or_else(|_| canonical_worktree.strip_prefix(canonical_root))
        .map_err(|_| anyhow::anyhow!("run worktree is outside the managed root"))?;
    let mut current = canonical_root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn validate_record_path(directory: &Path, path: &Path) -> Result<()> {
    let directory_metadata = fs::symlink_metadata(directory)?;
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        bail!("run record directory is unavailable")
    }
    let relative = path
        .strip_prefix(directory)
        .map_err(|_| anyhow::anyhow!("run record is outside its directory"))?;
    if relative.components().any(|component| {
        matches!(component, std::path::Component::ParentDir)
            || matches!(component, std::path::Component::RootDir)
    }) {
        bail!("run record path is unsafe")
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("run record is not a regular file")
    }
    Ok(())
}

fn validate_run_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 160
        || matches!(id, "." | "..")
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || !id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        bail!("invalid run id")
    }
    Ok(())
}

fn tail_bytes(bytes: &[u8], limit: usize) -> String {
    let start = bytes.len().saturating_sub(limit);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn rejects_path_traversal_run_ids() {
        assert!(validate_run_id("../secret").is_err());
        assert!(validate_run_id(".").is_err());
        assert!(validate_run_id("run/child").is_err());
        assert!(validate_run_id("run-1").is_ok());
    }

    #[test]
    fn truncates_logs_from_the_tail() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("run.log");
        fs::write(&path, b"0123456789").unwrap();
        assert_eq!(read_run_log(&path, 4).unwrap(), "6789");
    }

    #[test]
    fn truncates_git_output_from_the_tail() {
        assert_eq!(tail_bytes(b"0123456789", 3), "789");
    }

    #[test]
    fn worktrees_must_be_real_descendants_of_the_managed_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("worktrees");
        let valid = root.join("run-1");
        fs::create_dir_all(&valid).unwrap();
        assert_eq!(
            validate_worktree(&valid, &root).unwrap(),
            valid.canonicalize().unwrap()
        );
        #[cfg(target_os = "macos")]
        {
            let canonical_valid = valid.canonicalize().unwrap();
            assert_eq!(
                validate_worktree(&canonical_valid, &root).unwrap(),
                canonical_valid
            );
        }
        assert!(validate_worktree(temp.path(), &root).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp.path(), root.join("link")).unwrap();
            assert!(validate_worktree(&root.join("link"), &root).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn worktrees_reject_partial_windows_roots() {
        let worktree = Path::new(r"C:\worktrees\run-1");
        for root in [Path::new(r"\worktrees"), Path::new(r"C:worktrees")] {
            assert!(validate_worktree(worktree, root).is_err());
        }
        for partial in [
            Path::new(r"\worktrees\run-1"),
            Path::new(r"C:worktrees\run-1"),
        ] {
            assert!(validate_worktree(partial, Path::new(r"C:\worktrees")).is_err());
        }
    }

    #[test]
    fn run_record_paths_reject_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("runs");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("run.json");
        fs::write(&path, b"{}").unwrap();
        assert!(validate_record_path(&directory, &path).is_ok());
        #[cfg(unix)]
        {
            let target = temp.path().join("outside.json");
            fs::write(&target, b"{}").unwrap();
            fs::remove_file(&path).unwrap();
            std::os::unix::fs::symlink(&target, &path).unwrap();
            assert!(validate_record_path(&directory, &path).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn log_paths_reject_symlinked_files_outside_state() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let runs = state.join("runs");
        fs::create_dir_all(&runs).unwrap();
        let outside = temp.path().join("outside.log");
        fs::write(&outside, b"private\n").unwrap();
        let link = state.join("custom.log");
        symlink(&outside, &link).unwrap();
        let mut run = RunRecord::new(
            "run-1",
            neomax_core::Engine::Claude,
            "model",
            "prompt",
            "/profile",
            "/project",
            1,
        );
        run.log = Some(link);
        fs::write(runs.join("run-1.json"), serde_json::to_vec(&run).unwrap()).unwrap();
        let source = FilesystemPortalSource::new(temp.path(), &state);
        assert!(run_log(&source, "run-1", 400_000).unwrap().is_empty());
    }

    #[test]
    fn run_diff_returns_a_safe_error_for_an_out_of_root_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let runs = state.join("runs");
        fs::create_dir_all(&runs).unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let mut run = RunRecord::new(
            "run-1",
            neomax_core::Engine::Claude,
            "model",
            "prompt",
            "/profile",
            "/project",
            1,
        );
        run.worktree = Some(outside);
        fs::write(runs.join("run-1.json"), serde_json::to_vec(&run).unwrap()).unwrap();
        let source = FilesystemPortalSource::new(temp.path(), &state);
        let result = run_diff(&source, "run-1").unwrap();
        assert_eq!(result.error.as_deref(), Some("run worktree is unavailable"));
        assert!(result.patch.is_empty());
        assert!(result.worktree.is_none());
    }
}
