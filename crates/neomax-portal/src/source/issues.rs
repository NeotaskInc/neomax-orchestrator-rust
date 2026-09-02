use anyhow::Result;
use neomax_core::issues::{IssueStore, IssueStoreConfig};
use serde_json::Value;

use super::FilesystemPortalSource;

pub(crate) fn read_issues(source: &FilesystemPortalSource) -> Result<(Vec<Value>, usize)> {
    let store = IssueStore::with_config(
        source.paths.state.join("issues"),
        IssueStoreConfig {
            events_directory: Some(source.paths.issue_events.clone()),
            ..IssueStoreConfig::default()
        },
    );
    let view = store.list_with_diagnostics(None, None)?;
    let mut skipped = view.1.len();
    let issues = view
        .0
        .into_iter()
        .filter_map(|issue| match serde_json::to_value(issue) {
            Ok(value) => Some(value),
            Err(_) => {
                skipped = skipped.saturating_add(1);
                None
            }
        })
        .collect();
    Ok((issues, skipped))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use neomax_core::issues::{Issue, IssueStore};

    #[test]
    fn reads_issues_and_keeps_corrupt_optional_records_isolated() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        let directory = source.paths.state.join("issues");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("broken.json"), b"{").unwrap();
        let mut issue = Issue::new("ISSUE-1", "fixture issue", "fixture", 1);
        IssueStore::new(&directory).save_at(&mut issue, 1).unwrap();

        let (issues, skipped) = read_issues(&source).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0]["key"], "ISSUE-1");
        assert_eq!(skipped, 1);
    }

    #[test]
    fn missing_issue_directory_is_an_empty_optional_view() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        let (issues, skipped) = read_issues(&source).unwrap();
        assert!(issues.is_empty());
        assert_eq!(skipped, 0);
    }
}
