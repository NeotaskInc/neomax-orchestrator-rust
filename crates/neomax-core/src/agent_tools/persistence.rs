use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::manifest::AgentToolManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestStore {
    path: PathBuf,
}

impl ManifestStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, manifest: &AgentToolManifest) -> Result<()> {
        let bytes = manifest.json_bytes()?;
        crate::atomic::write_bytes_atomic(&self.path, &bytes)
    }

    pub fn write_canonical(&self) -> Result<AgentToolManifest> {
        let manifest = AgentToolManifest::canonical();
        self.write(&manifest)?;
        Ok(manifest)
    }

    pub fn read(&self) -> Result<AgentToolManifest> {
        if !self.path.is_file() {
            return Err(Error::NotFound(format!(
                "tool manifest does not exist: {}",
                self.path.display()
            )));
        }
        let manifest: AgentToolManifest = crate::atomic::read_json(&self.path)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn read_private_canonical(&self) -> Result<AgentToolManifest> {
        let metadata = std::fs::symlink_metadata(&self.path)?;
        if metadata.file_type().is_symlink() {
            return Err(Error::Conflict(format!(
                "tool manifest cannot be a symlink: {}",
                self.path.display()
            )));
        }
        crate::io::verify_private_path(&self.path)?;
        let manifest = self.read()?;
        if !manifest.is_canonical() {
            return Err(Error::Conflict(format!(
                "tool manifest is not canonical: {}",
                self.path.display()
            )));
        }
        Ok(manifest)
    }
}
