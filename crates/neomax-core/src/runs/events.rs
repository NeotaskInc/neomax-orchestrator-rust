use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::atomic::append_lines_locked;
use crate::io::{LocalFileSource, ReadLimits, read_file};
use crate::{Engine, Result};

use super::RunStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub ts: i64,
    pub run: String,
    pub event: String,
    #[serde(default = "default_event_engine")]
    pub engine: Engine,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub status: Option<RunStatus>,
    #[serde(default)]
    pub attempt: Option<u32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

pub struct EventStore {
    directory: PathBuf,
    legacy_directories: Vec<PathBuf>,
}

const EVENT_FILE_READ_MAX_BYTES: usize = 16 * 1024 * 1024;
const EVENT_FILE_READ_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_EVENTS_PER_READ: usize = 100_000;

impl EventStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            legacy_directories: Vec::new(),
        }
    }

    pub fn with_legacy_directory(
        directory: impl Into<PathBuf>,
        legacy_directory: impl Into<PathBuf>,
    ) -> Self {
        let directory = directory.into();
        let legacy_directory = legacy_directory.into();
        let legacy_directories = (directory != legacy_directory).then_some(legacy_directory);
        Self {
            directory,
            legacy_directories: legacy_directories.into_iter().collect(),
        }
    }

    pub fn append(&self, event: &RunEvent, at: DateTime<Utc>) -> Result<()> {
        let path = self.path_for(at);
        append_lines_locked(&path, &lock_path(&path), &[serde_json::to_vec(event)?])
    }

    pub fn path_for(&self, at: DateTime<Utc>) -> PathBuf {
        crate::io::event_partition::local_day_path(&self.directory, at)
    }

    pub fn read(&self, run_id: Option<&str>, limit: usize) -> Result<Vec<RunEvent>> {
        self.read_with_limits(
            run_id,
            limit,
            ReadLimits::new(EVENT_FILE_READ_MAX_BYTES, EVENT_FILE_READ_TIMEOUT)
                .expect("event read limits are valid"),
            MAX_EVENTS_PER_READ,
        )
    }

    fn read_with_limits(
        &self,
        run_id: Option<&str>,
        limit: usize,
        file_limits: ReadLimits,
        max_events: usize,
    ) -> Result<Vec<RunEvent>> {
        let mut files = BTreeSet::new();
        for directory in std::iter::once(&self.directory).chain(self.legacy_directories.iter()) {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            files.extend(
                entries.flatten().map(|entry| entry.path()).filter(|path| {
                    path.extension().and_then(|value| value.to_str()) == Some("jsonl")
                }),
            );
        }
        let mut events = Vec::new();
        let retention_limit = if limit == 0 {
            max_events
        } else {
            limit.min(max_events)
        };
        for path in files {
            let Ok(bytes) = read_file(&LocalFileSource, &path, file_limits) else {
                continue;
            };
            for line in bytes.split(|byte| *byte == b'\n') {
                let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
                    continue;
                };
                if value
                    .get("run")
                    .and_then(serde_json::Value::as_str)
                    .is_none()
                {
                    continue;
                }
                let Ok(event) = serde_json::from_value::<RunEvent>(value) else {
                    continue;
                };
                if run_id.is_none_or(|id| event.run == id) {
                    events.push(event);
                    if events.len() > retention_limit {
                        events.drain(..events.len() - retention_limit);
                    }
                }
            }
        }
        events.sort_by_key(|event| event.ts);
        Ok(events)
    }
}

fn default_event_engine() -> Engine {
    Engine::Claude
}

fn lock_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_limits_and_skips_malformed_event_lines() {
        let temp = tempfile::tempdir().unwrap();
        let store = EventStore::new(temp.path());
        let at = Utc::now();
        for run in ["a", "b", "a"] {
            store
                .append(
                    &RunEvent {
                        ts: at.timestamp(),
                        run: run.into(),
                        event: "updated".into(),
                        engine: Engine::Claude,
                        account: None,
                        status: None,
                        attempt: None,
                        extra: BTreeMap::new(),
                    },
                    at,
                )
                .unwrap();
        }
        fs::write(temp.path().join("broken.jsonl"), "{").unwrap();
        let events = store.read(Some("a"), 1).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].run, "a");
    }

    #[test]
    fn skips_oversized_event_files_without_unbounded_allocation() {
        let temp = tempfile::tempdir().unwrap();
        let store = EventStore::new(temp.path());
        fs::write(temp.path().join("oversized.jsonl"), b"0123456789").unwrap();
        let limits = ReadLimits::new(4, Duration::from_secs(1)).unwrap();
        assert!(
            store
                .read_with_limits(None, 0, limits, MAX_EVENTS_PER_READ)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn missing_event_directory_is_an_empty_view() {
        let temp = tempfile::tempdir().unwrap();
        let store = EventStore::new(temp.path().join("missing"));
        assert!(store.read(None, 0).unwrap().is_empty());
    }

    #[test]
    fn legacy_missing_engine_defaults_to_claude_and_unrelated_events_are_skipped() {
        let temp = tempfile::tempdir().unwrap();
        let store = EventStore::new(temp.path());
        fs::write(
            temp.path().join("legacy.jsonl"),
            concat!(
                "{\"ts\":1,\"run\":\"legacy\",\"event\":\"started\"}\n",
                "{\"ts\":1,\"plan_id\":\"plan\",\"event\":\"started\"}\n",
                "{\"ts\":1,\"issue\":\"ISSUE-1\",\"event\":\"opened\"}\n",
                "not-json\n",
            ),
        )
        .unwrap();
        let events = store.read(Some("legacy"), 0).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].engine, Engine::Claude);
    }

    #[test]
    fn new_run_events_use_local_day_and_read_the_legacy_mixed_root() {
        let temp = tempfile::tempdir().unwrap();
        let legacy = temp.path().join("events");
        let current = temp.path().join("events/runs");
        let store = EventStore::with_legacy_directory(&current, &legacy);
        let at = Utc::now();
        let event = RunEvent {
            ts: at.timestamp(),
            run: "run-1".into(),
            event: "started".into(),
            engine: Engine::Codex,
            account: None,
            status: None,
            attempt: None,
            extra: BTreeMap::new(),
        };
        store.append(&event, at).unwrap();
        assert!(store.path_for(at).is_file());

        let legacy_event = serde_json::json!({
            "ts": at.timestamp(),
            "run": "legacy-run",
            "event": "finished"
        });
        crate::atomic::append_line(
            &legacy.join("2000-01-01.jsonl"),
            &serde_json::to_vec(&legacy_event).unwrap(),
        )
        .unwrap();
        assert_eq!(store.read(Some("legacy-run"), 0).unwrap().len(), 1);
    }
}
