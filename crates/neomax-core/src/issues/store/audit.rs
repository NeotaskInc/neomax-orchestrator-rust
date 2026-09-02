use std::collections::BTreeMap;

use crate::Result;
use crate::atomic::append_line;

use super::super::types::{Issue, IssueEvent};
use super::core::IssueStore;

impl IssueStore {
    pub(super) fn audit_history_delta(&self, previous: Option<&Issue>, issue: &Issue) {
        let previous_len = previous.map_or(0, |value| value.history.len());
        for event in issue.history.iter().skip(previous_len) {
            let _ = self.append_audit(issue, &event.event, &event.extra, event.ts);
        }
    }

    pub fn append_event_at(
        &self,
        key: &str,
        event: &str,
        extra: BTreeMap<String, serde_json::Value>,
        now: i64,
    ) -> Result<Option<Issue>> {
        let path = self.issue_path(key)?;
        let lock = self.lock_path(&path);
        crate::atomic::with_exclusive_lock(&lock, || {
            let Some(mut issue) = self.load(key)? else {
                return Ok(None);
            };
            issue.history.push(IssueEvent {
                ts: now,
                event: event.into(),
                extra: extra.clone(),
            });
            self.save_at_unlocked(&mut issue, now, &path)?;
            Ok(Some(issue))
        })
    }

    pub(super) fn append_audit(
        &self,
        issue: &Issue,
        event: &str,
        extra: &BTreeMap<String, serde_json::Value>,
        now: i64,
    ) -> Result<()> {
        let Some(directory) = &self.config.events_directory else {
            return Ok(());
        };
        let mut entry = serde_json::Map::new();
        entry.insert("ts".into(), now.into());
        entry.insert("issue".into(), issue.key.clone().into());
        entry.insert("event".into(), event.into());
        entry.insert("project".into(), issue.project.clone().into());
        entry.insert("status".into(), issue.status.as_str().into());
        for (key, value) in extra {
            entry.insert(key.clone(), value.clone());
        }
        let line = serde_json::to_vec(&serde_json::Value::Object(entry))?;
        append_line(
            &crate::io::event_partition::local_day_path_from_timestamp(directory, now),
            &line,
        )
    }
}
