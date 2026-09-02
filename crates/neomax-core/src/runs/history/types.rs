use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Engine;

use crate::runs::{RunRecord, RunStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveOutcome {
    Archived,
    Spilled { pending: PathBuf },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySummary {
    pub id: String,
    pub engine: Engine,
    pub account: String,
    #[serde(
        default,
        alias = "acct_no",
        deserialize_with = "crate::runs::history::serde_helpers::deserialize_optional_u32"
    )]
    pub account_number: Option<u32>,
    pub status: RunStatus,
    pub prompt: Option<String>,
    pub branch: Option<String>,
    pub tag: Option<String>,
    pub goal: Option<String>,
    pub ultra: bool,
    pub opus: bool,
    pub effort: Option<String>,
    pub children: usize,
    pub attempt: u32,
    pub pr_url: Option<String>,
    pub started: i64,
    pub ended: Option<i64>,
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArchivedRun {
    pub run: RunRecord,
    pub log_path: Option<PathBuf>,
    pub status: RunStatus,
}

pub(super) fn status_name(status: RunStatus) -> &'static str {
    status.as_str()
}

pub(super) fn parse_status(value: &str) -> RunStatus {
    serde_json::from_value(serde_json::Value::String(value.to_string())).unwrap_or_default()
}

pub(super) fn truncate(value: &str, length: usize) -> String {
    value.chars().take(length).collect()
}

pub(super) fn project_from_repo(repo: Option<&str>) -> Option<String> {
    repo.and_then(|repo| {
        std::path::Path::new(repo)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}
