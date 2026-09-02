use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::atomic::append_lines_locked;
use crate::io::{LocalFileSource, ReadLimits, read_file};
use crate::{Error, Result};

use super::validation::validate_plan_id;

pub(super) const MAX_EVENT_FILE_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_EVENT_LINE_BYTES: usize = 2 * 1024 * 1024;
const EVENT_READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEvent {
    pub ts: i64,
    pub plan_id: String,
    pub event: String,
    #[serde(default)]
    pub status: Option<super::types::PlanStatus>,
    #[serde(default)]
    pub part_id: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl PlanEvent {
    pub fn new(plan_id: impl Into<String>, event: impl Into<String>, ts: i64) -> Result<Self> {
        let plan_id = plan_id.into();
        let event = event.into();
        let value = Self {
            ts,
            plan_id,
            event,
            status: None,
            part_id: None,
            error: None,
            extra: BTreeMap::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<()> {
        validate_plan_id(&self.plan_id)?;
        if self.event.trim().is_empty() {
            return Err(Error::InvalidArgument(
                "scheduler plan event name is empty".into(),
            ));
        }
        Ok(())
    }
}

pub struct PlanEventStore {
    directory: PathBuf,
    legacy_directories: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanEventDiagnostic {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct PlanEventView {
    pub events: Vec<PlanEvent>,
    pub diagnostics: Vec<PlanEventDiagnostic>,
}

impl PlanEventStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        let legacy_directory = directory.into();
        Self {
            directory: legacy_directory.join("scheduler"),
            legacy_directories: vec![legacy_directory],
        }
    }

    pub fn at_directory(directory: impl Into<PathBuf>) -> Self {
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

    pub fn directory(&self) -> &PathBuf {
        &self.directory
    }

    pub fn append(&self, event: &PlanEvent, at: DateTime<Utc>) -> Result<()> {
        event.validate()?;
        let path = self.path_for(at);
        append_lines_locked(&path, &lock_path(&path), &[serde_json::to_vec(event)?])
    }

    pub fn append_timestamp(&self, event: &PlanEvent, timestamp: i64) -> Result<()> {
        let at = DateTime::<Utc>::from_timestamp(timestamp, 0).ok_or_else(|| {
            Error::InvalidArgument(format!("invalid scheduler event timestamp {timestamp}"))
        })?;
        self.append(event, at)
    }

    pub fn read(&self, plan_id: Option<&str>, limit: usize) -> Result<Vec<PlanEvent>> {
        Ok(self.read_with_diagnostics(plan_id, limit)?.events)
    }

    pub fn read_with_diagnostics(
        &self,
        plan_id: Option<&str>,
        limit: usize,
    ) -> Result<PlanEventView> {
        if let Some(plan_id) = plan_id {
            validate_plan_id(plan_id)?;
        }
        let mut paths = Vec::new();
        for (directory, legacy) in std::iter::once((&self.directory, false)).chain(
            self.legacy_directories
                .iter()
                .map(|directory| (directory, true)),
        ) {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            paths.extend(
                entries
                    .filter_map(std::result::Result::ok)
                    .map(|entry| (entry.path(), legacy))
                    .filter(|(path, _)| {
                        path.extension().and_then(|value| value.to_str()) == Some("jsonl")
                    }),
            );
        }
        paths.sort_by(|left, right| left.0.cmp(&right.0));
        let mut view = PlanEventView::default();
        for (path, legacy) in paths {
            let bytes = match read_file(
                &LocalFileSource,
                &path,
                ReadLimits::new(MAX_EVENT_FILE_BYTES, EVENT_READ_TIMEOUT)?,
            ) {
                Ok(bytes) => bytes,
                Err(error) => {
                    view.diagnostics.push(PlanEventDiagnostic {
                        path,
                        line: None,
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            for (line_number, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
                if line.len() > MAX_EVENT_LINE_BYTES {
                    view.diagnostics.push(PlanEventDiagnostic {
                        path: path.clone(),
                        line: Some(line_number + 1),
                        message: format!(
                            "line {} exceeds the {} byte limit",
                            line_number + 1,
                            MAX_EVENT_LINE_BYTES
                        ),
                    });
                    continue;
                }
                let line = match str::from_utf8(line) {
                    Ok(line) => line,
                    Err(error) => {
                        view.diagnostics.push(PlanEventDiagnostic {
                            path: path.clone(),
                            line: Some(line_number + 1),
                            message: format!("line {} is not UTF-8: {error}", line_number + 1),
                        });
                        continue;
                    }
                };
                if line.trim().is_empty() {
                    continue;
                }
                let value = match serde_json::from_str::<serde_json::Value>(line) {
                    Ok(value) => value,
                    Err(error) => {
                        view.diagnostics.push(PlanEventDiagnostic {
                            path: path.clone(),
                            line: Some(line_number + 1),
                            message: format!("line {}: {error}", line_number + 1),
                        });
                        continue;
                    }
                };
                if value
                    .get("plan_id")
                    .and_then(serde_json::Value::as_str)
                    .is_none()
                {
                    if !legacy {
                        view.diagnostics.push(PlanEventDiagnostic {
                            path: path.clone(),
                            line: Some(line_number + 1),
                            message: format!(
                                "line {} does not contain a scheduler plan_id",
                                line_number + 1
                            ),
                        });
                    }
                    continue;
                }
                let event = match serde_json::from_value::<PlanEvent>(value) {
                    Ok(event) => event,
                    Err(error) => {
                        view.diagnostics.push(PlanEventDiagnostic {
                            path: path.clone(),
                            line: Some(line_number + 1),
                            message: format!("line {}: {error}", line_number + 1),
                        });
                        continue;
                    }
                };
                if let Err(error) = event.validate() {
                    view.diagnostics.push(PlanEventDiagnostic {
                        path: path.clone(),
                        line: Some(line_number + 1),
                        message: format!("line {}: {error}", line_number + 1),
                    });
                    continue;
                }
                if plan_id.is_none_or(|value| event.plan_id == value) {
                    view.events.push(event);
                }
            }
        }
        view.events.sort_by_key(|event| event.ts);
        if limit != 0 && view.events.len() > limit {
            view.events.drain(..view.events.len() - limit);
        }
        Ok(view)
    }

    pub fn path_for(&self, at: DateTime<Utc>) -> PathBuf {
        crate::io::event_partition::local_day_path(&self.directory, at)
    }
}

fn lock_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.to_string_lossy()))
}
