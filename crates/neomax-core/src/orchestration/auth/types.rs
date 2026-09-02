use std::path::{Path, PathBuf};

use crate::usage::UsageCacheStore;
use crate::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationPaths {
    pub backup_dir: PathBuf,
    pub rotation_log: PathBuf,
    pub usage_cache_dir: Option<PathBuf>,
}

impl RotationPaths {
    pub fn new(backup_dir: impl Into<PathBuf>, rotation_log: impl Into<PathBuf>) -> Self {
        Self {
            backup_dir: backup_dir.into(),
            rotation_log: rotation_log.into(),
            usage_cache_dir: None,
        }
    }

    pub fn with_usage_cache_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.usage_cache_dir = Some(directory.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationOperation {
    Copy,
    Swap,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationEffects {
    pub engine: Engine,
    pub operation: RotationOperation,
    pub destination: PathBuf,
    pub source: Option<PathBuf>,
    pub backup_paths: Vec<PathBuf>,
    pub invalidated_cache_paths: Vec<PathBuf>,
}

impl RotationEffects {
    pub(crate) fn for_profile(
        engine: Engine,
        operation: RotationOperation,
        destination: impl Into<PathBuf>,
        source: Option<PathBuf>,
        backup_paths: Vec<PathBuf>,
        usage_cache_dir: Option<&Path>,
        extra_profiles: impl IntoIterator<Item = PathBuf>,
    ) -> Self {
        let destination = destination.into();
        let mut profiles = vec![destination.clone()];
        profiles.extend(extra_profiles);
        let invalidated_cache_paths = if matches!(engine, Engine::Claude | Engine::Codex) {
            usage_cache_dir
                .into_iter()
                .flat_map(|directory| {
                    let store = UsageCacheStore::new(directory);
                    profiles
                        .iter()
                        .flat_map(move |profile| store.cache_paths(engine, profile))
                })
                .collect()
        } else {
            Vec::new()
        };
        Self {
            engine,
            operation,
            destination,
            source,
            backup_paths,
            invalidated_cache_paths,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileSnapshot {
    pub credential: Option<Vec<u8>>,
    pub identity: Option<Vec<u8>>,
    pub auth: Option<Vec<u8>>,
}

pub(crate) fn profile_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("profile")
        .to_string()
}

pub(crate) fn same_profile(left: &Path, right: &Path) -> bool {
    if crate::io::is_rooted_but_not_absolute(left) || crate::io::is_rooted_but_not_absolute(right) {
        return true;
    }
    absolute_path(left) == absolute_path(right)
}

pub(crate) fn absolute_path(path: &Path) -> PathBuf {
    if crate::io::is_rooted_but_not_absolute(path) {
        return path.to_path_buf();
    }
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;

    #[cfg(windows)]
    #[test]
    fn partial_root_profiles_fail_closed_in_identity_comparisons() {
        let safe = Path::new(r"C:\profiles\safe");
        for raw in [r"\rooted", r"C:drive-relative"] {
            let partial = Path::new(raw);
            assert_eq!(absolute_path(partial), partial);
            assert!(same_profile(partial, safe));
            assert!(same_profile(safe, partial));
        }
    }
}
