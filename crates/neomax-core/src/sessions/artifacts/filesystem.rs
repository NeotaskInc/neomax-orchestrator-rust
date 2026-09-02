use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::io::{read_file, BoundedIoError, LocalFileSource, ReadLimits};
use crate::Result;

use super::index::ArtifactIndex;
use super::matching::matches_kind;
use super::source::ArtifactSource;
use super::types::{Artifact, ArtifactKind, ArtifactLocator};

#[derive(Debug, Clone)]
pub struct FsArtifactSource {
    pub max_bytes: usize,
}

impl Default for FsArtifactSource {
    fn default() -> Self {
        Self {
            max_bytes: 128 * 1024 * 1024,
        }
    }
}

impl FsArtifactSource {
    pub fn new(max_bytes: usize) -> Self {
        Self { max_bytes }
    }

    pub fn index_with_home(
        &self,
        profile: &Path,
        home: &Path,
        cutoff: i64,
    ) -> Result<ArtifactIndex> {
        let mut artifacts = self.index(profile, cutoff)?.into_artifacts();
        let database = super::super::opencode::database_path(profile, home);
        if database.is_file()
            && !artifacts.iter().any(|artifact| {
                artifact.kind == ArtifactKind::OpenCodeDatabase && artifact.path == database
            })
        {
            if let Ok(metadata) = fs::metadata(&database) {
                let modified = modified_epoch(&metadata);
                if modified >= cutoff
                    && usize::try_from(metadata.len()).unwrap_or(usize::MAX) <= self.max_bytes
                {
                    artifacts.push(ArtifactLocator {
                        profile: profile.to_path_buf(),
                        path: database,
                        kind: ArtifactKind::OpenCodeDatabase,
                        modified,
                        bytes: metadata.len(),
                    });
                }
            }
        }
        Ok(ArtifactIndex::new(profile, artifacts))
    }

    pub fn read(&self, locator: &ArtifactLocator) -> Result<Option<Artifact>> {
        self.load(
            &locator.profile,
            locator.path.clone(),
            locator.kind,
            locator.modified,
        )
    }

    fn load(
        &self,
        profile: &Path,
        path: PathBuf,
        kind: ArtifactKind,
        cutoff: i64,
    ) -> Result<Option<Artifact>> {
        let modified = path
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|stamp| stamp.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or_default();
        if modified < cutoff {
            return Ok(None);
        }
        let metadata = fs::metadata(&path)?;
        if self.max_bytes == 0 || self.max_bytes == usize::MAX {
            return Ok(None);
        }
        if usize::try_from(metadata.len()).unwrap_or(usize::MAX) > self.max_bytes {
            return Ok(None);
        }
        let bytes = match read_file(
            &LocalFileSource,
            &path,
            ReadLimits::new(self.max_bytes, std::time::Duration::from_secs(30))?,
        ) {
            Ok(bytes) => bytes,
            Err(BoundedIoError::NotFound { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        Ok(Some(Artifact {
            profile: profile.to_path_buf(),
            path: path.clone(),
            kind,
            modified,
            bytes,
        }))
    }
}

impl ArtifactSource for FsArtifactSource {
    fn discover(&self, profile: &Path, kind: ArtifactKind, cutoff: i64) -> Result<Vec<Artifact>> {
        let mut paths = Vec::new();
        for entry in WalkDir::new(profile)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            let path = entry.path();
            if path.is_file() && matches_kind(path, profile, kind) {
                paths.push(path.to_path_buf());
            }
        }
        paths.sort();
        paths
            .into_iter()
            .filter_map(|path| match self.load(profile, path, kind, cutoff) {
                Ok(Some(artifact)) => Some(Ok(artifact)),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    fn index(&self, profile: &Path, cutoff: i64) -> Result<ArtifactIndex> {
        let mut artifacts = Vec::new();
        for entry in WalkDir::new(profile)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let metadata = match fs::metadata(path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let modified = modified_epoch(&metadata);
            if modified < cutoff {
                continue;
            }
            if usize::try_from(metadata.len()).unwrap_or(usize::MAX) > self.max_bytes {
                continue;
            }
            for kind in ArtifactKind::ALL {
                if matches_kind(path, profile, kind) {
                    artifacts.push(ArtifactLocator {
                        profile: profile.to_path_buf(),
                        path: path.to_path_buf(),
                        kind,
                        modified,
                        bytes: metadata.len(),
                    });
                }
            }
        }
        Ok(ArtifactIndex::new(profile, artifacts))
    }
}

fn modified_epoch(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|stamp| stamp.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
