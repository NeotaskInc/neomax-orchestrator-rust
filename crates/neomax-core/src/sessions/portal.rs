use std::cmp::Reverse;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::types::{SessionKind, SessionRecord, SessionSummary};

fn saturating_sum(values: impl Iterator<Item = u64>) -> u64 {
    values.fold(0, u64::saturating_add)
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PortalSnapshot {
    pub generated_at: i64,
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
    #[serde(default)]
    pub subagents: Vec<SessionRecord>,
    #[serde(default)]
    pub summary: SessionSummary,
}

impl PortalSnapshot {
    pub fn all_records(&self) -> impl Iterator<Item = &SessionRecord> {
        self.sessions.iter().chain(self.subagents.iter())
    }
}

pub fn portal_snapshot(
    now: i64,
    records: impl IntoIterator<Item = SessionRecord>,
) -> PortalSnapshot {
    let mut sessions = Vec::new();
    let mut subagents = Vec::new();
    for mut record in records {
        record.update_age(now);
        if record.is_child() {
            subagents.push(record);
        } else {
            sessions.push(record);
        }
    }
    sessions.sort_by_key(|record| Reverse(record.last_active.unwrap_or_default()));
    subagents.sort_by_key(|record| Reverse(record.last_active.unwrap_or_default()));
    let all = sessions.iter().chain(subagents.iter()).collect::<Vec<_>>();
    let files = all
        .iter()
        .flat_map(|record| record.files.iter().map(|file| file.path.as_str()))
        .collect::<BTreeSet<_>>();
    let summary = SessionSummary {
        sessions: count(all.len()),
        mains: count(sessions.len()),
        subagents: count(subagents.len()),
        active: count(all.iter().filter(|record| record.active).count()),
        working: count(all.iter().filter(|record| record.working).count()),
        input: saturating_sum(all.iter().map(|record| record.tokens.input)),
        output: saturating_sum(all.iter().map(|record| record.tokens.output)),
        reasoning: saturating_sum(all.iter().map(|record| record.tokens.reasoning)),
        cache_read: saturating_sum(all.iter().map(|record| record.tokens.cache_read)),
        cache_write: saturating_sum(all.iter().map(|record| record.tokens.cache_write)),
        cost: all.iter().map(|record| record.tokens.cost).sum(),
        requests: saturating_sum(all.iter().map(|record| record.requests)),
        completions: saturating_sum(all.iter().map(|record| record.completions)),
        errors: saturating_sum(all.iter().map(|record| record.errors)),
        rate_limits: saturating_sum(all.iter().map(|record| record.rate_limits)),
        tool_calls: saturating_sum(all.iter().map(|record| record.tool_calls)),
        tool_errors: saturating_sum(all.iter().map(|record| record.tool_errors)),
        files: count(files.len()),
    };
    PortalSnapshot {
        generated_at: now,
        sessions,
        subagents,
        summary,
    }
}

pub fn flatten_native_children(
    records: impl IntoIterator<Item = SessionRecord>,
) -> Vec<SessionRecord> {
    let mut output = Vec::new();
    for record in records {
        flatten_record(record, &mut output);
    }
    output
}

fn flatten_record(mut record: SessionRecord, output: &mut Vec<SessionRecord>) {
    let children = std::mem::take(&mut record.children);
    output.push(record);
    for mut child in children {
        child.kind = SessionKind::NativeSubagent;
        flatten_record(child, output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;

    #[test]
    fn portal_snapshot_separates_mains_and_subagents_and_counts_usage() {
        let mut main = SessionRecord::with_identity("main", Engine::Claude, "acct");
        main.last_active = Some(99);
        main.active = true;
        main.tokens.input = 3;
        let mut child = SessionRecord::with_identity("child", Engine::Claude, "acct");
        child.kind = SessionKind::NativeSubagent;
        child.parent_id = Some("main".into());
        child.last_active = Some(98);
        child.tokens.output = 4;
        let snapshot = portal_snapshot(100, [main, child]);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.subagents.len(), 1);
        assert_eq!(snapshot.summary.sessions, 2);
        assert_eq!(snapshot.summary.input, 3);
        assert_eq!(snapshot.summary.output, 4);
    }

    #[test]
    fn flatten_native_children_handles_nested_agents() {
        let mut root = SessionRecord::with_identity("root", Engine::Claude, "acct");
        let mut child = SessionRecord::with_identity("child", Engine::Claude, "acct");
        child.parent_id = Some("root".into());
        let mut grandchild = SessionRecord::with_identity("grandchild", Engine::Claude, "acct");
        grandchild.parent_id = Some("child".into());
        child.children.push(grandchild);
        root.children.push(child);
        let rows = flatten_native_children([root]);
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["root", "child", "grandchild"]
        );
        assert!(rows.iter().skip(1).all(SessionRecord::is_child));
    }

    #[test]
    fn portal_summary_saturates_usage_counters() {
        let mut first = SessionRecord::with_identity("first", Engine::Claude, "account");
        first.tokens.input = u64::MAX;
        first.tokens.output = u64::MAX;
        first.requests = u64::MAX;
        first.tool_calls = u64::MAX;
        let mut second = SessionRecord::with_identity("second", Engine::Claude, "account");
        second.tokens.input = 1;
        second.tokens.output = 1;
        second.requests = 1;
        second.tool_calls = 1;

        let summary = portal_snapshot(100, [first, second]).summary;
        assert_eq!(summary.input, u64::MAX);
        assert_eq!(summary.output, u64::MAX);
        assert_eq!(summary.requests, u64::MAX);
        assert_eq!(summary.tool_calls, u64::MAX);
    }
}
