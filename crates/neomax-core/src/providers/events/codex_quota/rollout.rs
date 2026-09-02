use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;
use walkdir::WalkDir;

use crate::io::{read_file_range, FileSource, LocalFileSource, ReadLimits};

use super::types::CodexQuotaRefreshResult;

pub(super) const CODEX_ROLLOUT_TAIL_BYTES: usize = 4 * 1024 * 1024;
const CODEX_ROLLOUT_READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Reads one bounded local Codex rollout tail without invoking Codex.
pub fn refresh_from_rollout(
    profile: &Path,
    session_id: Option<&str>,
    observed_at: f64,
) -> crate::Result<Option<CodexQuotaRefreshResult>> {
    let Some(path) = newest_rollout(profile, session_id) else {
        return Ok(None);
    };
    let metadata = match LocalFileSource.metadata(&path) {
        Ok(metadata) if metadata.regular => metadata,
        Ok(_) => return Ok(None),
        Err(crate::io::BoundedIoError::NotFound { .. }) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let length = usize::try_from(metadata.len).unwrap_or(usize::MAX);
    let tail_length = length.min(CODEX_ROLLOUT_TAIL_BYTES);
    if tail_length == 0 {
        return Ok(None);
    }
    let offset = metadata.len.saturating_sub(tail_length as u64);
    let limits = ReadLimits::new(CODEX_ROLLOUT_TAIL_BYTES, CODEX_ROLLOUT_READ_TIMEOUT)
        .expect("Codex rollout read limits are valid");
    let bytes = match read_file_range(&LocalFileSource, &path, offset, tail_length, limits) {
        Ok(bytes) => bytes,
        Err(crate::io::BoundedIoError::NotFound { .. }) => {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    Ok(bytes
        .split(|byte| *byte == b'\n')
        .rev()
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .filter_map(|event| rollout_rate_limits(&event))
        .find_map(|value| CodexQuotaRefreshResult::from_value(value, observed_at)))
}

fn newest_rollout(profile: &Path, session_id: Option<&str>) -> Option<PathBuf> {
    let sessions = profile.join("sessions");
    let mut newest: Option<(Option<SystemTime>, PathBuf)> = None;
    for entry in WalkDir::new(sessions)
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() || !is_rollout(path) {
            continue;
        }
        if let Some(session_id) = session_id {
            let path_text = path.to_string_lossy();
            if !path_text.contains(session_id) {
                continue;
            }
        }
        let modified = fs::metadata(path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        let candidate = (modified, path.to_path_buf());
        if newest
            .as_ref()
            .is_none_or(|current| candidate.0 > current.0)
        {
            newest = Some(candidate);
        }
    }
    newest.map(|(_, path)| path)
}

fn is_rollout(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
}

fn rollout_rate_limits(event: &Value) -> Option<Value> {
    let payload = event
        .get("payload")
        .or_else(|| event.get("params"))
        .unwrap_or(event);
    payload
        .get("rate_limits")
        .or_else(|| payload.get("rateLimits"))
        .cloned()
}
