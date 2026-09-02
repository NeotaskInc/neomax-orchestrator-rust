use std::path::{Path, PathBuf};

use crate::Engine;

use super::types::{engine_for_kind, ArtifactKind, ArtifactLocator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactIndex {
    profile: PathBuf,
    artifacts: Vec<ArtifactLocator>,
}

pub type ProviderArtifactIndex = ArtifactIndex;

impl ArtifactIndex {
    pub fn new(profile: impl Into<PathBuf>, mut artifacts: Vec<ArtifactLocator>) -> Self {
        artifacts
            .sort_by(|left, right| left.path.cmp(&right.path).then(left.kind.cmp(&right.kind)));
        Self {
            profile: profile.into(),
            artifacts,
        }
    }

    pub fn profile(&self) -> &Path {
        &self.profile
    }

    pub fn iter(&self) -> impl Iterator<Item = &ArtifactLocator> {
        self.artifacts.iter()
    }

    pub fn by_kind(&self, kind: ArtifactKind) -> impl Iterator<Item = &ArtifactLocator> {
        self.artifacts
            .iter()
            .filter(move |artifact| artifact.kind == kind)
    }

    pub fn by_engine(&self, engine: Engine) -> impl Iterator<Item = &ArtifactLocator> {
        self.artifacts
            .iter()
            .filter(move |artifact| engine_for_kind(artifact.kind) == Some(engine))
    }

    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    pub fn into_artifacts(self) -> Vec<ArtifactLocator> {
        self.artifacts
    }
}
