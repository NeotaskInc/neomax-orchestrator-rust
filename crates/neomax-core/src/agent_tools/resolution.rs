use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableInputs {
    pub current_exe: Option<PathBuf>,
    pub install_bin: Option<PathBuf>,
}

impl ExecutableInputs {
    pub fn new(current_exe: Option<PathBuf>, install_bin: Option<PathBuf>) -> Self {
        Self {
            current_exe,
            install_bin,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutableSource {
    CurrentExecutable,
    InstallBin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedExecutable {
    pub path: PathBuf,
    pub source: ExecutableSource,
}

pub fn resolve_executable(inputs: &ExecutableInputs) -> Result<ResolvedExecutable> {
    let candidates = [
        (
            inputs.current_exe.as_deref(),
            ExecutableSource::CurrentExecutable,
        ),
        (inputs.install_bin.as_deref(), ExecutableSource::InstallBin),
    ];

    for (candidate, source) in candidates {
        let Some(candidate) = candidate else {
            continue;
        };
        if !candidate.is_absolute() {
            continue;
        }
        if !candidate.is_file() {
            continue;
        }
        let path = fs::canonicalize(candidate).map_err(|error| {
            Error::InvalidArgument(format!(
                "cannot resolve Neomax executable {}: {error}",
                candidate.display()
            ))
        })?;
        if !is_executable(&path) {
            continue;
        }
        return Ok(ResolvedExecutable { path, source });
    }

    Err(Error::NotFound(
        "Neomax executable was not found in the explicit current-exe or install-bin inputs".into(),
    ))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(extension.to_ascii_lowercase().as_str(), "exe" | "com")
                })
    }
    #[cfg(all(not(unix), not(windows)))]
    {
        path.is_file()
    }
}
