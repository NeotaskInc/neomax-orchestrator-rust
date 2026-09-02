use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};
use neomax_core::io::{
    FileSource, LocalFileSource, ReadLimits, is_rooted_but_not_absolute, read_file_range,
};
use neomax_core::runs::{HistoryStore, RunStore};
use serde::Serialize;
use serde_json::Value;

use super::RunLifecycleReport;
use super::options;
use crate::context::RuntimeContext;
use crate::error;

const MAX_LOG_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 16 * 1024;

#[derive(Debug, Serialize)]
pub(crate) struct LogReport {
    pub id: String,
    pub path: String,
    pub entries: Vec<LogEntry>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum LogEntry {
    Text {
        text: String,
    },
    Tool {
        name: String,
        input: Value,
    },
    Result {
        subtype: Option<String>,
        text: String,
    },
    Event {
        event_type: Option<String>,
        raw: Value,
    },
}

pub(crate) fn log(context: &RuntimeContext, args: &[String]) -> Result<RunLifecycleReport> {
    let id = error::usage(options::run_id(args))?;
    let store = RunStore::new(&context.paths.runs);
    let path = match store.load(&id) {
        Ok(run) => run.log,
        Err(_) if !store.path(&id).exists() => history_store(context)
            .get(&id)?
            .and_then(|run| run.log_path),
        Err(error) => return Err(error.into()),
    }
    .ok_or_else(|| anyhow::anyhow!("no log for {id}"))?;
    Ok(RunLifecycleReport::Log(read_log(context, &id, &path)?))
}

pub(crate) fn read_log(context: &RuntimeContext, id: &str, path: &Path) -> Result<LogReport> {
    let path = allowed_log_path(context, path)?;
    let source = LocalFileSource;
    let metadata = source.metadata(&path).map_err(anyhow::Error::msg)?;
    if !metadata.regular {
        bail!("run log is not a regular file: {}", path.display());
    }
    let maximum = u64::try_from(MAX_LOG_BYTES).unwrap_or(u64::MAX);
    let offset = metadata.len.saturating_sub(maximum);
    let limits =
        ReadLimits::new(MAX_LOG_BYTES, Duration::from_secs(10)).map_err(anyhow::Error::msg)?;
    let bytes = read_file_range(
        &source,
        &path,
        offset,
        (metadata.len - offset) as usize,
        limits,
    )
    .map_err(anyhow::Error::msg)?;
    let truncated = offset > 0;
    let text = String::from_utf8_lossy(&bytes);
    let entries = text.lines().flat_map(parse_line).collect();
    Ok(LogReport {
        id: id.to_owned(),
        path: path.to_string_lossy().into_owned(),
        entries,
        truncated,
    })
}

pub(crate) fn read_archived_log(
    context: &RuntimeContext,
    id: &str,
    path: &Path,
) -> Result<LogReport> {
    read_log(context, id, path)
}

fn allowed_log_path(context: &RuntimeContext, path: &Path) -> Result<PathBuf> {
    validate_absolute_path(path, "run log")?;
    for root in [&context.paths.logs, &context.paths.history_logs] {
        validate_absolute_path(root, "Neomax log directory")?;
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| anyhow::anyhow!("run log does not exist: {}", path.display()))?;
    let roots = [&context.paths.logs, &context.paths.history_logs];
    let allowed = roots.iter().any(|root| {
        std::fs::canonicalize(root)
            .map(|root| canonical.starts_with(root))
            .unwrap_or(false)
    });
    if !allowed {
        bail!("run log is outside the Neomax log directories");
    }
    Ok(canonical)
}

fn validate_absolute_path(path: &Path, label: &str) -> Result<()> {
    if is_rooted_but_not_absolute(path) {
        bail!(
            "{label} must not be rooted without an absolute prefix: {}",
            path.display()
        );
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!(
            "{label} cannot contain parent-directory traversal: {}",
            path.display()
        );
    }
    if !path.is_absolute() {
        bail!("{label} must be absolute: {}", path.display());
    }
    Ok(())
}

fn history_store(context: &RuntimeContext) -> HistoryStore {
    HistoryStore::new(
        &context.paths.history_db,
        &context.paths.logs,
        &context.paths.history_logs,
        &context.paths.history_pending,
    )
}

fn parse_line(line: &str) -> Vec<LogEntry> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    match object.get("type").and_then(Value::as_str) {
        Some("assistant") => parse_assistant(object.get("message")),
        Some("result") => vec![LogEntry::Result {
            subtype: object
                .get("subtype")
                .and_then(Value::as_str)
                .map(str::to_owned),
            text: truncate_value(object.get("result").unwrap_or(&Value::Null)),
        }],
        _ => vec![LogEntry::Event {
            event_type: object
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_owned),
            raw: truncate_json(value),
        }],
    }
}

fn parse_assistant(message: Option<&Value>) -> Vec<LogEntry> {
    let Some(blocks) = message
        .and_then(|value| value.get("content"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut texts = Vec::new();
    let mut entries = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    texts.push(text.to_owned());
                }
            }
            Some("tool_use") => {
                entries.push(LogEntry::Tool {
                    name: block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned(),
                    input: truncate_json(block.get("input").cloned().unwrap_or(Value::Null)),
                });
            }
            _ => {}
        }
    }
    if !texts.is_empty() {
        entries.insert(
            0,
            LogEntry::Text {
                text: truncate(&texts.join("\n"), MAX_ENTRY_BYTES),
            },
        );
    }
    entries
}

fn truncate_value(value: &Value) -> String {
    truncate(&value.to_string(), MAX_ENTRY_BYTES)
}

fn truncate_json(value: Value) -> Value {
    let encoded = value.to_string();
    if encoded.len() <= MAX_ENTRY_BYTES {
        return value;
    }
    Value::String(truncate(&encoded, MAX_ENTRY_BYTES))
}

fn truncate(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes.saturating_sub(16);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}... [truncated]", &value[..end])
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn persisted_log_paths_require_absolute_non_traversing_paths() {
        let temp = tempfile::tempdir().expect("temporary root");
        let absolute = temp.path().join("logs/run.log");
        assert!(validate_absolute_path(Path::new("../run.log"), "run log").is_err());
        assert!(validate_absolute_path(Path::new("logs/run.log"), "run log").is_err());
        assert!(validate_absolute_path(&absolute, "run log").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn persisted_log_paths_reject_windows_partial_roots() {
        for path in [Path::new(r"\logs\run.log"), Path::new(r"C:logs\run.log")] {
            assert!(validate_absolute_path(path, "run log").is_err());
        }
    }
}
