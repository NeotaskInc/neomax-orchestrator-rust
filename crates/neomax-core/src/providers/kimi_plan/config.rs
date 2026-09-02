use std::path::Path;
use std::time::Duration;

use crate::Result;
use crate::io::{BoundedIoError, LocalFileSource, ReadLimits, read_file};

pub(super) const MAX_CONFIG_BYTES: usize = 2 * 1024 * 1024;

const CONFIG_READ_TIMEOUT: Duration = Duration::from_secs(5);

const ALLOWED_TOOLS: [&str; 12] = [
    "Read",
    "Grep",
    "Glob",
    "ReadMediaFile",
    "WebSearch",
    "FetchURL",
    "Agent",
    "AgentSwarm",
    "TodoList",
    "TaskList",
    "TaskOutput",
    "WaitFor",
];

pub(super) fn read(profile: &Path) -> Result<String> {
    let limits = ReadLimits::new(MAX_CONFIG_BYTES, CONFIG_READ_TIMEOUT)
        .expect("Kimi config read limits are valid");
    match read_file(&LocalFileSource, &profile.join("config.toml"), limits) {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|error| crate::Error::Message(format!("Kimi config is not UTF-8: {error}"))),
        Err(BoundedIoError::NotFound { .. }) => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn with_read_only_tools(source: &str) -> Result<String> {
    let tools = format!(
        "[tools]\nenabled = {}\n",
        serde_json::to_string(&ALLOWED_TOOLS)?
    );
    let config = replace_section(source, "tools", &tools);
    Ok(config.trim_start().to_owned())
}

fn replace_section(source: &str, section: &str, replacement: &str) -> String {
    let header = format!("[{section}]");
    let mut offset = 0;
    let mut start = None;
    let mut end = source.len();
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        if start.is_none() && trimmed == header {
            start = Some(offset);
        } else if start.is_some() && trimmed.starts_with('[') {
            end = offset;
            break;
        }
        offset += line.len();
    }
    match start {
        Some(start) => format!("{}{}{}", &source[..start], replacement, &source[end..]),
        None if source.trim().is_empty() => replacement.into(),
        None => format!("{}\n\n{replacement}", source.trim_end()),
    }
}
