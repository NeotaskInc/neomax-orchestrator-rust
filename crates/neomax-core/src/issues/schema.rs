use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Result;

use super::claim::IssueClaim;
use super::event::IssueEvent;
use super::mirror::IssueMirror;
use super::status::IssueStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestLink {
    pub url: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub status: IssueStatus,
    #[serde(default)]
    pub claim: Option<IssueClaim>,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub updated: i64,
    #[serde(default)]
    pub repos: BTreeMap<String, IssueMirror>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repositories: Vec<String>,
    #[serde(default)]
    pub runs: Vec<String>,
    #[serde(default, rename = "prs")]
    pub pull_requests: BTreeMap<String, String>,
    #[serde(default)]
    pub history: Vec<IssueEvent>,
    #[serde(skip)]
    pub new_record: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Issue {
    pub fn new(
        key: impl Into<String>,
        title: impl Into<String>,
        project: impl Into<String>,
        now: i64,
    ) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            body: String::new(),
            project: project.into(),
            severity: None,
            labels: Vec::new(),
            status: IssueStatus::Open,
            claim: None,
            fingerprint: None,
            created: now,
            updated: now,
            repos: BTreeMap::new(),
            repositories: Vec::new(),
            runs: Vec::new(),
            pull_requests: BTreeMap::new(),
            history: Vec::new(),
            new_record: false,
            extra: BTreeMap::new(),
        }
    }

    pub fn transition(&mut self, next: IssueStatus, now: i64) -> Result<()> {
        if !self.status.can_transition_to(&next) {
            return Err(self.status.transition_error(&next, &self.key));
        }
        self.status = next;
        self.updated = now;
        Ok(())
    }

    pub fn link_run(&mut self, run: impl Into<String>) -> bool {
        let run = run.into();
        if run.is_empty() || self.runs.contains(&run) {
            return false;
        }
        self.runs.push(run);
        true
    }

    pub fn link_pull_request(&mut self, repository: impl Into<String>, url: impl Into<String>) {
        self.pull_requests.insert(repository.into(), url.into());
    }

    pub fn repository_names(&self) -> impl Iterator<Item = &String> {
        self.repos.keys()
    }
}
