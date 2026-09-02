use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;

use crate::atomic::{read_json, with_exclusive_lock, write_json_atomic};
use crate::{Error, Result};

use super::super::types::{Issue, IssueStatus};

const DEFAULT_CLAIM_TTL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueLoadDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct IssueStoreConfig {
    pub claim_ttl: Duration,
    pub events_directory: Option<PathBuf>,
}

impl Default for IssueStoreConfig {
    fn default() -> Self {
        Self {
            claim_ttl: DEFAULT_CLAIM_TTL,
            events_directory: None,
        }
    }
}

pub struct IssueStore {
    pub(super) directory: PathBuf,
    pub(super) config: IssueStoreConfig,
}

impl IssueStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            config: IssueStoreConfig::default(),
        }
    }

    pub fn with_config(directory: impl Into<PathBuf>, config: IssueStoreConfig) -> Self {
        Self {
            directory: directory.into(),
            config,
        }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn load(&self, key: &str) -> Result<Option<Issue>> {
        Ok(self.load_with_diagnostic(key)?.0)
    }

    pub fn load_strict(&self, key: &str) -> Result<Option<Issue>> {
        let path = self.issue_path(key)?;
        match read_json(&path) {
            Ok(issue) => Ok(Some(issue)),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn load_with_diagnostic(
        &self,
        key: &str,
    ) -> Result<(Option<Issue>, Option<IssueLoadDiagnostic>)> {
        let path = self.issue_path(key)?;
        match read_json(&path) {
            Ok(issue) => Ok((Some(issue), None)),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok((None, None))
            }
            Err(Error::InvalidState { path, message }) => {
                Ok((None, Some(IssueLoadDiagnostic { path, message })))
            }
            Err(Error::Message(message)) if is_bounded_read_failure(&message) => {
                Ok((None, Some(IssueLoadDiagnostic { path, message })))
            }
            Err(error) => Err(error),
        }
    }

    pub fn list(&self, project: Option<&str>, status: Option<&IssueStatus>) -> Result<Vec<Issue>> {
        Ok(self.list_with_diagnostics(project, status)?.0)
    }

    pub fn list_with_diagnostics(
        &self,
        project: Option<&str>,
        status: Option<&IssueStatus>,
    ) -> Result<(Vec<Issue>, Vec<IssueLoadDiagnostic>)> {
        let _directory_guard = crate::io::PathGuard::for_directory(&self.directory)?;
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), Vec::new()));
            }
            Err(error) => return Err(error.into()),
        };
        let mut issues = Vec::new();
        let mut diagnostics = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let issue = match read_json::<Issue>(&path) {
                Ok(issue) => issue,
                Err(Error::InvalidState { path, message }) => {
                    diagnostics.push(IssueLoadDiagnostic { path, message });
                    continue;
                }
                Err(Error::Message(message)) if is_bounded_read_failure(&message) => {
                    diagnostics.push(IssueLoadDiagnostic { path, message });
                    continue;
                }
                Err(_) => continue,
            };
            if project.is_some_and(|name| issue.project != name)
                || status.is_some_and(|expected| issue.status != *expected)
            {
                continue;
            }
            issues.push(issue);
        }
        issues.sort_by(|left, right| {
            left.created
                .cmp(&right.created)
                .then_with(|| left.key.cmp(&right.key))
        });
        diagnostics.sort_by(|left, right| left.path.cmp(&right.path));
        Ok((issues, diagnostics))
    }

    pub fn save(&self, issue: &mut Issue) -> Result<()> {
        self.save_at(issue, Utc::now().timestamp())
    }

    pub fn repair(&self, issue: &mut Issue) -> Result<()> {
        self.repair_at(issue, Utc::now().timestamp())
    }

    pub fn save_at(&self, issue: &mut Issue, now: i64) -> Result<()> {
        self.validate_key(&issue.key)?;
        let path = self.issue_path(&issue.key)?;
        let lock = self.lock_path(&path);
        with_exclusive_lock(&lock, || {
            if path.exists() {
                read_json::<Issue>(&path)?;
            }
            self.save_at_unlocked(issue, now, &path)
        })
    }

    pub fn repair_at(&self, issue: &mut Issue, now: i64) -> Result<()> {
        self.validate_key(&issue.key)?;
        let path = self.issue_path(&issue.key)?;
        let lock = self.lock_path(&path);
        with_exclusive_lock(&lock, || self.save_at_unlocked(issue, now, &path))
    }

    pub(super) fn save_at_unlocked(&self, issue: &mut Issue, now: i64, path: &Path) -> Result<()> {
        if issue.created == 0 {
            issue.created = now;
        }
        issue.updated = now;
        let _directory_guard = crate::io::PathGuard::ensure_directory(&self.directory)?;
        let previous = read_json::<Issue>(path).ok();
        write_json_atomic(path, issue)?;
        self.audit_history_delta(previous.as_ref(), issue);
        Ok(())
    }

    pub fn update_at<F>(&self, key: &str, now: i64, update: F) -> Result<Option<Issue>>
    where
        F: FnOnce(&mut Issue) -> Result<()>,
    {
        let path = self.issue_path(key)?;
        let lock = self.lock_path(&path);
        with_exclusive_lock(&lock, || {
            let Some(mut issue) = self.load(key)? else {
                return Ok(None);
            };
            update(&mut issue)?;
            let path = self.issue_path(&issue.key)?;
            self.save_at_unlocked(&mut issue, now, &path)?;
            Ok(Some(issue))
        })
    }

    pub(super) fn issue_path(&self, key: &str) -> Result<PathBuf> {
        self.validate_key(key)?;
        Ok(self.directory.join(format!("{key}.json")))
    }

    pub(super) fn lock_path(&self, path: &Path) -> PathBuf {
        PathBuf::from(format!("{}.lock", path.to_string_lossy()))
    }

    pub(super) fn validate_key(&self, key: &str) -> Result<()> {
        if key.is_empty()
            || !key
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
        {
            return Err(Error::InvalidArgument(format!("invalid issue key {key:?}")));
        }
        Ok(())
    }
}

fn is_bounded_read_failure(message: &str) -> bool {
    message.contains(" exceeded its ")
}
