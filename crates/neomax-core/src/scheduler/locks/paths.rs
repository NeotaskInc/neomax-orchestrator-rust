use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub(super) struct RepoLockPaths {
    directory: PathBuf,
}

impl RepoLockPaths {
    pub(super) fn new(locks_root: &Path, repo: &Path) -> io::Result<Self> {
        let absolute = absolute_path(repo)?;
        let digest = Sha256::digest(absolute.to_string_lossy().as_bytes());
        let key = digest
            .iter()
            .take(6)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let directory = locks_root.join(key);
        let _directory_guard = crate::io::PathGuard::ensure_directory(&directory)?;
        Ok(Self { directory })
    }

    pub(super) fn path(&self, area: &str) -> PathBuf {
        self.directory.join(format!("{}.lock", area_filename(area)))
    }

    pub(super) fn transaction_lock(&self) -> PathBuf {
        self.directory.join(".areas.guard")
    }

    pub(super) fn all(&self) -> io::Result<Vec<PathBuf>> {
        let _directory_guard = crate::io::PathGuard::for_directory(&self.directory)?;
        let mut paths = fs::read_dir(&self.directory)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("lock"))
            .collect::<Vec<_>>();
        paths.sort();
        Ok(paths)
    }
}

pub(super) fn area_filename(area: &str) -> String {
    let value = if area.is_empty() { "*" } else { area };
    // Windows does not permit `*` in a filename. Keep the historical Unix
    // name while giving the global area a stable, legal Windows name.
    #[cfg(windows)]
    let value = if value == "*" { "__global__" } else { value };
    value.replace(['/', '\\'], "__").replace('\0', "_")
}

fn absolute_path(path: &Path) -> io::Result<PathBuf> {
    if crate::io::is_rooted_but_not_absolute(path) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "repository path is rooted but not absolute: {}",
                path.display()
            ),
        ));
    }
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()?.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_compatible_short_sha_path_and_safe_area_names() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let paths = RepoLockPaths::new(temp.path().join("locks").as_path(), &repo).unwrap();
        let expected = Sha256::digest(repo.to_string_lossy().as_bytes());
        let key = expected
            .iter()
            .take(6)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert!(paths
            .path("apps/web")
            .starts_with(temp.path().join("locks").join(key)));
        assert!(paths.path("apps/web").ends_with("apps__web.lock"));
        #[cfg(windows)]
        assert!(paths.path("*").ends_with("__global__.lock"));
        #[cfg(windows)]
        assert!(paths.path("").ends_with("__global__.lock"));
        #[cfg(unix)]
        assert!(paths.path("*").ends_with("*.lock"));
        assert!(paths.path("../unsafe").starts_with(&paths.directory));
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_paths_that_depend_on_the_current_drive_directory() {
        for path in [
            Path::new(r"\rooted-repository"),
            Path::new(r"C:drive-relative"),
        ] {
            let error = absolute_path(path).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        }
    }
}
