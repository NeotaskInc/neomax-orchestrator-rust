use std::path::Path;

use anyhow::Result;
use neomax_core::io::{FileSource, LocalFileSource, ReadLimits, read_file};
use serde::de::DeserializeOwned;

pub(crate) const MAX_STATE_JSON_BYTES: u64 = 16 * 1024 * 1024;
const STATE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub(crate) fn load<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let metadata = match LocalFileSource.metadata(path) {
        Ok(metadata) => metadata,
        Err(neomax_core::io::BoundedIoError::NotFound { .. }) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let max_bytes = usize::try_from(MAX_STATE_JSON_BYTES)
        .map_err(|_| anyhow::anyhow!("state JSON limit does not fit this platform"))?;
    if metadata.len > MAX_STATE_JSON_BYTES {
        anyhow::bail!("state JSON exceeds {} bytes", MAX_STATE_JSON_BYTES)
    }
    let bytes = read_file(
        &LocalFileSource,
        path,
        ReadLimits::new(max_bytes, STATE_READ_TIMEOUT)?,
    )?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn missing_state_is_empty_and_oversized_state_is_rejected() {
        let temp = tempfile::tempdir().unwrap();
        assert!(
            load::<serde_json::Value>(&temp.path().join("missing"))
                .unwrap()
                .is_none()
        );
        let path = temp.path().join("large.json");
        fs::write(&path, vec![b'x'; (MAX_STATE_JSON_BYTES + 1) as usize]).unwrap();
        assert!(load::<serde_json::Value>(&path).is_err());
    }
}
