use crate::Result;

use super::{Issue, IssueStatus, IssueStore};

pub fn normalize_title(title: &str) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in title.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(character);
            pending_space = false;
        } else {
            pending_space = true;
        }
    }
    normalized
}

pub fn issue_fingerprint(title: &str, project: Option<&str>, repositories: &[String]) -> String {
    let mut repositories = repositories.to_vec();
    repositories.sort();
    repositories.dedup();
    format!(
        "{}::{}::{}",
        project.unwrap_or_default(),
        repositories.join(","),
        normalize_title(title)
    )
}

pub fn find_open_duplicate(
    store: &IssueStore,
    fingerprint: &str,
    project: Option<&str>,
) -> Result<Option<Issue>> {
    Ok(store.list(project, None)?.into_iter().find(|issue| {
        !matches!(issue.status, IssueStatus::Done | IssueStatus::Closed)
            && issue.fingerprint.as_deref() == Some(fingerprint)
    }))
}
