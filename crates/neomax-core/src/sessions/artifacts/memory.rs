use std::path::Path;

use crate::Result;

use super::source::ArtifactSource;
use super::types::{Artifact, ArtifactKind};

#[derive(Debug, Clone, Default)]
pub struct MemoryArtifactSource {
    artifacts: Vec<Artifact>,
}

impl MemoryArtifactSource {
    pub fn new(artifacts: impl IntoIterator<Item = Artifact>) -> Self {
        Self {
            artifacts: artifacts.into_iter().collect(),
        }
    }

    pub fn push(&mut self, artifact: Artifact) {
        self.artifacts.push(artifact);
    }
}

impl ArtifactSource for MemoryArtifactSource {
    fn discover(&self, profile: &Path, kind: ArtifactKind, cutoff: i64) -> Result<Vec<Artifact>> {
        Ok(self
            .artifacts
            .iter()
            .filter(|artifact| {
                artifact.profile == profile && artifact.kind == kind && artifact.modified >= cutoff
            })
            .cloned()
            .collect())
    }
}
