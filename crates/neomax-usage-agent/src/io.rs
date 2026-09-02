use std::io::{self, Read};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use neomax_core::io::{FileSource, LocalFileSource, ReadLimits, read_file, read_file_range};

pub(crate) const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_CREDENTIAL_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_COMMAND_OUTPUT_BYTES: usize = 128 * 1024;
pub(crate) const MAX_HTTP_BODY_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_SOURCE_BYTES_PER_SWEEP: usize = 4 * 1024 * 1024;
pub(crate) const LOCAL_FILE_READ_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let limits = read_limits(maximum)?;
    read_file(&LocalFileSource, path, limits)
        .map_err(|error| anyhow::anyhow!("read {}: {error}", path.display()))
}

pub(crate) fn file_len(path: &Path) -> Result<u64> {
    let metadata = LocalFileSource
        .metadata(path)
        .map_err(|error| anyhow::anyhow!("stat {}: {error}", path.display()))?;
    if !metadata.regular {
        bail!("{} is not a regular file", path.display());
    }
    Ok(metadata.len)
}

pub(crate) fn read_range(path: &Path, offset: u64, length: usize) -> Result<Vec<u8>> {
    if length == 0 {
        return Ok(Vec::new());
    }
    let limits = ReadLimits::new(length, LOCAL_FILE_READ_TIMEOUT)
        .map_err(|error| anyhow::anyhow!("read range {}: {error}", path.display()))?;
    read_file_range(&LocalFileSource, path, offset, length, limits)
        .map_err(|error| anyhow::anyhow!("read range {}: {error}", path.display()))
}

fn read_limits(maximum: u64) -> Result<ReadLimits> {
    let maximum =
        usize::try_from(maximum).context("local read limit does not fit this platform")?;
    ReadLimits::new(maximum, LOCAL_FILE_READ_TIMEOUT)
        .map_err(|error| anyhow::anyhow!("invalid local read limit: {error}"))
}

pub(crate) fn read_string(path: &Path, maximum: u64) -> Result<String> {
    String::from_utf8(read_bounded(path, maximum)?)
        .map_err(|error| anyhow::anyhow!("{} is not valid UTF-8: {error}", path.display()))
}

pub(crate) fn read_capped<R: Read>(mut reader: R, maximum: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    let mut limited = reader.by_ref().take(maximum as u64 + 1);
    limited.read_to_end(&mut bytes)?;
    let exceeded = bytes.len() > maximum;
    if exceeded {
        bytes.truncate(maximum);
    }
    Ok((bytes, exceeded))
}
