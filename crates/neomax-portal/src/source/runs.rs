use std::fs;
use std::path::Path;

use anyhow::Result;
use neomax_core::io::{LocalFileSource, ReadLimits, read_file};
use neomax_core::runs::RunRecord;

const MAX_RECORD_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RECORDS: usize = 10_000;
const RECORD_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub(crate) fn read_records(path: &Path) -> Result<(Vec<RunRecord>, usize)> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(error) => return Err(error.into()),
    };
    let mut records = Vec::new();
    let mut skipped = 0;
    let mut scanned = 0;
    let mut entries = entries;
    while scanned < MAX_RECORDS {
        let Some(entry) = entries.next().and_then(|result| result.ok()) else {
            break;
        };
        scanned += 1;
        if records.len() >= MAX_RECORDS {
            skipped += 1;
            break;
        }
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_RECORD_BYTES
        {
            skipped += 1;
            continue;
        }
        match read_record_file(&path) {
            Ok(record) => records.push(record),
            Err(_) => skipped += 1,
        }
    }
    if scanned == MAX_RECORDS && entries.next().is_some() {
        skipped += 1;
    }
    records.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((records, skipped))
}

pub(crate) fn load_record(directory: &Path, id: &str) -> Result<RunRecord> {
    let path = directory.join(format!("{id}.json"));
    super::files::validate_record_path(directory, &path)?;
    read_record_file(&path)
}

fn read_record_file(path: &Path) -> Result<RunRecord> {
    let max_bytes = usize::try_from(MAX_RECORD_BYTES)
        .map_err(|_| anyhow::anyhow!("run record limit does not fit this platform"))?;
    let bytes = match read_file(
        &LocalFileSource,
        path,
        ReadLimits::new(max_bytes, RECORD_READ_TIMEOUT)?,
    ) {
        Ok(bytes) => bytes,
        Err(neomax_core::io::BoundedIoError::NotFound { .. }) => {
            return Err(anyhow::anyhow!("run record does not exist"));
        }
        Err(error) => return Err(error.into()),
    };
    if bytes.len() > max_bytes {
        anyhow::bail!("run record exceeds {} bytes", MAX_RECORD_BYTES)
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_loader_skips_oversized_and_malformed_records() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("bad.json"), b"{").unwrap();
        fs::write(
            temp.path().join("large.json"),
            vec![b'x'; (MAX_RECORD_BYTES + 1) as usize],
        )
        .unwrap();
        let (records, skipped) = read_records(temp.path()).unwrap();
        assert!(records.is_empty());
        assert_eq!(skipped, 2);
    }

    #[test]
    fn large_run_directories_stop_scanning_at_the_entry_bound() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..(MAX_RECORDS + 10) {
            fs::write(temp.path().join(format!("record-{index}.json")), b"{").unwrap();
        }
        let (records, skipped) = read_records(temp.path()).unwrap();
        assert!(records.is_empty());
        assert!(skipped <= MAX_RECORDS + 1);
        assert!(skipped >= MAX_RECORDS);
    }
}
