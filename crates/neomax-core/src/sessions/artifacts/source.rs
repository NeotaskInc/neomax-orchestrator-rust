use std::path::Path;

use crate::Result;

use super::index::ArtifactIndex;
use super::types::{Artifact, ArtifactKind};

pub trait ArtifactSource: Send + Sync {
    fn discover(&self, profile: &Path, kind: ArtifactKind, cutoff: i64) -> Result<Vec<Artifact>>;

    fn index(&self, profile: &Path, cutoff: i64) -> Result<ArtifactIndex> {
        let mut artifacts = Vec::new();
        for kind in ArtifactKind::ALL {
            artifacts.extend(
                self.discover(profile, kind, cutoff)?
                    .into_iter()
                    .map(|artifact| artifact.locator()),
            );
        }
        Ok(ArtifactIndex::new(profile, artifacts))
    }
}
