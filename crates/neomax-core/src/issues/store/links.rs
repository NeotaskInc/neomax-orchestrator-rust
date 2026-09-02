use std::collections::BTreeMap;

use crate::Result;

use super::super::types::{Issue, IssueEvent, IssueStatus};
use super::core::IssueStore;

impl IssueStore {
    pub fn set_status_at(&self, key: &str, status: IssueStatus, now: i64) -> Result<Option<Issue>> {
        self.update_at(key, now, |issue| {
            let target = status.clone();
            issue.transition(target, now)?;
            issue.history.push(IssueEvent {
                ts: now,
                event: "status".into(),
                extra: BTreeMap::from([("to".into(), status.as_str().into())]),
            });
            Ok(())
        })
    }

    pub fn link_run_at(
        &self,
        key: &str,
        run: impl Into<String>,
        now: i64,
    ) -> Result<Option<Issue>> {
        let run = run.into();
        self.update_at(key, now, |issue| {
            if issue.link_run(&run) {
                issue.history.push(IssueEvent {
                    ts: now,
                    event: "linked-run".into(),
                    extra: BTreeMap::from([("run".into(), run.clone().into())]),
                });
            }
            Ok(())
        })
    }

    pub fn link_pull_request_at(
        &self,
        key: &str,
        repository: impl Into<String>,
        url: impl Into<String>,
        now: i64,
    ) -> Result<Option<Issue>> {
        let repository = repository.into();
        let url = url.into();
        self.update_at(key, now, |issue| {
            issue.link_pull_request(&repository, &url);
            issue.history.push(IssueEvent {
                ts: now,
                event: "linked-pr".into(),
                extra: BTreeMap::from([
                    ("repo".into(), repository.clone().into()),
                    ("url".into(), url.clone().into()),
                ]),
            });
            Ok(())
        })
    }

    pub fn find_open_duplicate(
        &self,
        fingerprint: &str,
        project: Option<&str>,
    ) -> Result<Option<Issue>> {
        super::super::fingerprint::find_open_duplicate(self, fingerprint, project)
    }
}
