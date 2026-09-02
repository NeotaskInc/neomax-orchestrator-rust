use std::fs;
use std::path::Path;

use crate::agent_tools::{ManifestStore, ToolManifest};
use crate::{Error, Result};

pub fn ensure_private_canonical(path: &Path) -> Result<ToolManifest> {
    let store = ManifestStore::new(path);
    let manifest = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(Error::Conflict(format!(
                "Neomax tool manifest cannot be a symlink: {}",
                path.display()
            )));
        }
        Ok(_) => {
            require_private_file(path)?;
            store.read()?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => store.write_canonical()?,
        Err(error) => return Err(error.into()),
    };
    if !manifest.is_canonical() {
        return Err(Error::Conflict(format!(
            "Neomax tool manifest is not canonical: {}",
            path.display()
        )));
    }
    require_private_file(path)?;
    Ok(manifest)
}

fn require_private_file(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(Error::InvalidState {
            path: path.to_path_buf(),
            message: "Neomax tool manifest is not a regular file".into(),
        });
    }
    crate::io::verify_private_path(path)
}
