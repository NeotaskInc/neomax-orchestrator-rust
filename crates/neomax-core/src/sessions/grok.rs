use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{Engine, Result};

use super::activity::{classify_activity, ActivityInput, ActivityState};
use super::artifacts::{Artifact, ArtifactKind, ArtifactSource};
use super::filters::{apply_context, DiscoveryContext};
use super::headers::timestamp_epoch;
use super::types::{SessionKind, SessionRecord};

mod updates;
mod usage;

pub use usage::{extract_usage, parse_usage_line, GrokUsageRecord};

pub fn discover<S: ArtifactSource>(
    source: &S,
    profile: &Path,
    account: &str,
    context: &DiscoveryContext,
    cutoff: i64,
) -> Result<Vec<SessionRecord>> {
    let summaries = source.discover(profile, ArtifactKind::GrokSummary, cutoff)?;
    let updates = source.discover(profile, ArtifactKind::GrokUpdates, cutoff)?;
    let mut by_directory = BTreeMap::<PathBuf, Artifact>::new();
    for update in updates {
        if let Some(directory) = update.path.parent() {
            by_directory.insert(directory.to_path_buf(), update);
        }
    }
    let mut rows = Vec::new();
    for summary in summaries {
        let update = by_directory.get(summary.path.parent().unwrap_or(profile));
        rows.extend(parse_summary(&summary, update, account, context));
    }
    rows.sort_by_key(|record| std::cmp::Reverse(record.last_active.unwrap_or_default()));
    Ok(rows)
}

pub fn parse_summary(
    summary_artifact: &Artifact,
    updates_artifact: Option<&Artifact>,
    account: &str,
    context: &DiscoveryContext,
) -> Vec<SessionRecord> {
    let summary = serde_json::from_slice::<Value>(&summary_artifact.bytes).unwrap_or(Value::Null);
    let info = summary.get("info").unwrap_or(&Value::Null);
    let id = info
        .get("id")
        .or_else(|| summary.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            summary_artifact
                .path
                .parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                .unwrap_or("session")
                .into()
        });
    let parent_id = summary
        .get("parent_session_id")
        .or_else(|| info.get("parent_session_id"))
        .and_then(Value::as_str)
        .map(Into::into);
    let child = parent_id.is_some()
        || summary
            .get("session_kind")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind.starts_with("subagent"));
    let mut stats = updates_artifact.map(updates::parse).unwrap_or_default();
    let last_active = summary
        .get("last_active_at")
        .or_else(|| summary.get("updated_at"))
        .and_then(timestamp_epoch)
        .unwrap_or(summary_artifact.modified)
        .max(stats.last_active);
    let model = summary
        .get("current_model_id")
        .or_else(|| summary.get("model"))
        .and_then(Value::as_str)
        .map(Into::into)
        .or(stats.model.take())
        .or_else(|| Some(crate::providers::catalog::GROK_DEFAULT_MODEL.into()));
    let mut record = SessionRecord::with_identity(id.clone(), Engine::Grok, account);
    record.kind = if child {
        SessionKind::NativeSubagent
    } else {
        SessionKind::Main
    };
    record.parent_id = parent_id;
    record.cwd = info
        .get("cwd")
        .or_else(|| summary.get("cwd"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    record.branch = summary
        .get("head_branch")
        .or_else(|| summary.get("branch"))
        .and_then(Value::as_str)
        .map(Into::into);
    record.model = model;
    record.label = summary
        .get("generated_title")
        .or_else(|| summary.get("session_summary"))
        .or_else(|| summary.get("last_turn_summary"))
        .and_then(Value::as_str)
        .map(Into::into);
    record.started = summary.get("created_at").and_then(timestamp_epoch);
    record.last_active = Some(last_active);
    record.active = classify_activity(ActivityInput {
        now: context.now,
        last_modified: last_active,
        active_window: context.active_window,
        progress: stats.last_active > 0,
        ..ActivityInput::default()
    }) == ActivityState::Active;
    record.working = record.active;
    record.done = !record.active;
    record.tokens = stats.tokens;
    record.requests = stats.requests;
    record.completions = stats.completions;
    record.errors = stats.errors;
    record.rate_limits = stats.rate_limits;
    record.tool_calls = stats.tool_calls;
    record.tool_errors = stats.tool_errors;
    record.files = stats.files.into_values().collect();
    record.extra = summary
        .as_object()
        .map(|object| {
            super::artifacts::flatten_extra(
                object,
                &[
                    "info",
                    "id",
                    "parent_session_id",
                    "session_kind",
                    "cwd",
                    "head_branch",
                    "current_model_id",
                    "model",
                    "generated_title",
                    "session_summary",
                    "last_turn_summary",
                    "created_at",
                    "updated_at",
                    "last_active_at",
                ],
            )
        })
        .unwrap_or_default();
    if !apply_context(&mut record, context).unwrap_or(false) {
        return Vec::new();
    }
    let mut rows = vec![record.clone()];
    if !child {
        for agent in stats.agents.into_values() {
            let mut child = SessionRecord::with_identity(agent.id.clone(), Engine::Grok, account);
            child.kind = SessionKind::NativeSubagent;
            child.parent_id = Some(record.id.clone());
            child.cwd = record.cwd.clone();
            child.project = record.project.clone();
            child.branch = record.branch.clone();
            child.model = agent.model.or_else(|| record.model.clone());
            child.label = agent.label;
            child.started = record.started;
            child.last_active = Some(
                agent
                    .last_active
                    .max(record.last_active.unwrap_or_default()),
            );
            child.active = agent.status == "running"
                && context
                    .now
                    .saturating_sub(child.last_active.unwrap_or_default())
                    <= context.active_window;
            child.working = child.active;
            child.done = !child.active;
            child.tool_calls = agent.tool_calls;
            child.errors = agent.errors;
            if apply_context(&mut child, context).unwrap_or(false) {
                rows.push(child);
            }
        }
    }
    rows
}
