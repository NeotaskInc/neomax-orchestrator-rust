use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{Engine, Result};

use super::activity::ActivityState;
use super::artifacts::{json_lines, ArtifactKind, ArtifactSource};
use super::filters::{apply_context, DiscoveryContext};
use super::headers::{claude_head_meta, claude_tail_activity, session_id_from_path, workflow_id};
use super::subagents;
use super::types::{FileActivity, SessionKind, SessionRecord};

pub fn discover<S: ArtifactSource>(
    source: &S,
    profile: &Path,
    account: &str,
    context: &DiscoveryContext,
    cutoff: i64,
) -> Result<Vec<SessionRecord>> {
    let mut records = source
        .discover(profile, ArtifactKind::ClaudeMain, cutoff)?
        .into_iter()
        .filter_map(|artifact| parse_main(&artifact, account, context))
        .collect::<Vec<_>>();
    records.extend(
        source
            .discover(profile, ArtifactKind::ClaudeSubagent, cutoff)?
            .into_iter()
            .filter_map(|artifact| parse_subagent(&artifact, account, context)),
    );
    records.sort_by_key(|record| std::cmp::Reverse(record.last_active.unwrap_or_default()));
    Ok(records)
}

pub fn parse_main(
    artifact: &super::artifacts::Artifact,
    account: &str,
    context: &DiscoveryContext,
) -> Option<SessionRecord> {
    let (head, tail) = artifact.head_tail(256 * 1024, 64 * 1024);
    let meta = claude_head_meta(&head);
    let id = meta
        .session_id
        .clone()
        .unwrap_or_else(|| session_id_from_path(&artifact.path, Engine::Claude));
    let activity =
        claude_tail_activity(&tail, context.now, artifact.modified, context.active_window);
    let mut record = SessionRecord::with_identity(id, Engine::Claude, account);
    record.model = model_from_head(&head);
    record.cwd = meta.cwd.map(PathBuf::from);
    record.branch = meta.branch;
    record.slug = meta.slug;
    record.label = meta.label;
    record.started = meta.started.or(Some(artifact.modified));
    record.last_active = Some(artifact.modified);
    record.active = activity.is_active();
    record.working = record.active;
    record.done = activity == ActivityState::Idle;
    record.tokens = super::headers::claude_token_usage(&artifact.text());
    record.files = claude_files(&artifact.text());
    record.extra = meta.extra;
    if !apply_context(&mut record, context).ok()? {
        return None;
    }
    Some(record)
}

pub fn parse_subagent(
    artifact: &super::artifacts::Artifact,
    account: &str,
    context: &DiscoveryContext,
) -> Option<SessionRecord> {
    let (head, tail) = artifact.head_tail(16 * 1024, 64 * 1024);
    let meta = claude_head_meta(&head);
    let parent = meta
        .session_id
        .clone()
        .or_else(|| parent_from_path(&artifact.path));
    let id = artifact
        .path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("subagent")
        .to_string();
    let activity =
        claude_tail_activity(&tail, context.now, artifact.modified, context.active_window);
    let mut record = SessionRecord::with_identity(id, Engine::Claude, account);
    record.kind = SessionKind::TranscriptSubagent;
    record.parent_id = parent;
    record.workflow = workflow_id(&artifact.path);
    record.model = model_from_head(&head);
    record.cwd = meta.cwd.map(PathBuf::from);
    record.branch = meta.branch;
    record.slug = meta.slug;
    record.label = meta.label;
    record.started = meta.started.or(Some(artifact.modified));
    record.last_active = Some(artifact.modified);
    record.active = activity.is_active();
    record.working = record.active;
    record.done = activity == ActivityState::Idle;
    record.tokens = super::headers::claude_token_usage(&artifact.text());
    record.files = claude_files(&artifact.text());
    record.extra = meta.extra;
    if !apply_context(&mut record, context).ok()? {
        return None;
    }
    Some(record)
}

pub fn normalize_children(
    parent: &mut SessionRecord,
    children: impl IntoIterator<Item = SessionRecord>,
) {
    for child in children {
        let child = subagents::normalize_child(child, parent);
        parent.children.push(child);
    }
}

fn model_from_head(head: &str) -> Option<String> {
    json_lines(head).find_map(|event| {
        event
            .get("model")
            .and_then(Value::as_str)
            .or_else(|| {
                event
                    .get("message")
                    .and_then(|message| message.get("model"))
                    .and_then(Value::as_str)
            })
            .map(str::to_string)
    })
}

fn parent_from_path(path: &Path) -> Option<String> {
    let mut components = path.components().rev();
    components.next()?;
    components.next()?;
    components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .map(str::to_string)
}

fn claude_files(text: &str) -> Vec<FileActivity> {
    let mut files = BTreeMap::<String, FileActivity>::new();
    for event in json_lines(text) {
        let Some(content) = event
            .get("message")
            .and_then(|message| message.get("content"))
        else {
            continue;
        };
        for block in content.as_array().into_iter().flatten() {
            if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            let name = block
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let input = block.get("input").unwrap_or(&Value::Null);
            let path = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .and_then(Value::as_str);
            let Some(path) = path else {
                continue;
            };
            let row = files
                .entry(path.to_string())
                .or_insert_with(|| FileActivity {
                    path: path.into(),
                    ..FileActivity::default()
                });
            row.ops = row.ops.saturating_add(1);
            if name.eq_ignore_ascii_case("edit") {
                row.adds = row.adds.saturating_add(line_count(
                    input.get("new_string").or_else(|| input.get("newString")),
                ));
                row.dels = row.dels.saturating_add(line_count(
                    input.get("old_string").or_else(|| input.get("oldString")),
                ));
            } else if name.eq_ignore_ascii_case("write") {
                row.adds = row.adds.saturating_add(line_count(input.get("content")));
            }
        }
    }
    files.into_values().collect()
}

fn line_count(value: Option<&Value>) -> u64 {
    value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(|text| text.lines().count() as u64)
        .unwrap_or_default()
}
