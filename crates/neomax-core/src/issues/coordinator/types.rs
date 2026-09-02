use std::path::PathBuf;

use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryTarget {
    pub name: String,
    pub path: PathBuf,
}

impl RepositoryTarget {
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
        }
    }
}

pub trait RepositoryCatalog: Send + Sync {
    fn repositories(&self, project: &str) -> Result<Vec<RepositoryTarget>>;
}

#[derive(Debug, Clone)]
pub struct CrossRepoIssueInput {
    pub title: String,
    pub body: String,
    pub project: String,
    pub repositories: Option<Vec<String>>,
    pub severity: Option<String>,
    pub labels: Vec<String>,
    pub key: Option<String>,
    pub fingerprint: Option<String>,
    pub force_new: bool,
    pub now: i64,
}

impl CrossRepoIssueInput {
    pub fn new(
        title: impl Into<String>,
        body: impl Into<String>,
        project: impl Into<String>,
        now: i64,
    ) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            project: project.into(),
            repositories: None,
            severity: None,
            labels: Vec::new(),
            key: None,
            fingerprint: None,
            force_new: false,
            now,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MirrorRequest {
    pub key: String,
    pub title: String,
    pub body: String,
    pub project: String,
    pub labels: Vec<String>,
}
