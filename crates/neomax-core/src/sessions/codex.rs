use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{Engine, Result};

use super::activity::ActivityState;
use super::artifacts::{ArtifactKind, ArtifactSource, json_lines};
use super::filters::{DiscoveryContext, apply_context};
use super::headers::{
    codex_head_meta, codex_session_live, codex_tail_activity, session_id_from_path,
};
use super::subagents;
use super::types::{SessionKind, SessionRecord, SessionTokens};

pub fn discover<S: ArtifactSource>(
    source: &S,
    profile: &Path,
    account: &str,
    context: &DiscoveryContext,
    cutoff: i64,
) -> Result<Vec<SessionRecord>> {
    let mut rows = Vec::new();
    source.visit(
        profile,
        ArtifactKind::CodexRollout,
        cutoff,
        &mut |artifact| {
            if let Some(record) = parse_rollout(&artifact, account, context) {
                rows.push(record);
            }
        },
    )?;
    rows.sort_by_key(|record| std::cmp::Reverse(record.last_active.unwrap_or_default()));
    Ok(rows)
}

pub fn parse_rollout(
    artifact: &super::artifacts::Artifact,
    account: &str,
    context: &DiscoveryContext,
) -> Option<SessionRecord> {
    let (head, tail) = artifact.head_tail(256 * 1024, 64 * 1024);
    let meta = codex_head_meta(&head);
    let id = meta
        .session_id
        .clone()
        .unwrap_or_else(|| session_id_from_path(&artifact.path, Engine::Codex));
    let live = codex_session_live(&tail, context.now, artifact.modified, context.active_window);
    let working = codex_tail_activity(&tail, context.now, artifact.modified, context.active_window);
    let mut record = SessionRecord::with_identity(id, Engine::Codex, account);
    record.parent_id = meta.parent_id;
    record.kind = if record.parent_id.is_some() {
        SessionKind::NativeSubagent
    } else {
        SessionKind::Main
    };
    record.model = model(&head);
    record.cwd = meta.cwd.map(PathBuf::from);
    record.branch = meta.branch;
    record.label = meta.label;
    record.started = meta.started.or(Some(artifact.modified));
    record.last_active = Some(artifact.modified);
    record.active = live == ActivityState::Active;
    record.working = working == ActivityState::Active;
    record.done = live == ActivityState::Stopped;
    let (tokens, activity) = tokens(&artifact.text());
    record.tokens = tokens;
    record.model = activity.model.clone().or(record.model);
    record.activity = Some(activity);
    record.children = subagents::child_records_from_events(json_lines(&artifact.text()), &record);
    record.extra = meta.extra;
    if !apply_context(&mut record, context).ok()? {
        return None;
    }
    Some(record)
}

fn model(text: &str) -> Option<String> {
    json_lines(text).find_map(|event| {
        event
            .get("model")
            .or_else(|| {
                event
                    .get("payload")
                    .and_then(|payload| payload.get("model"))
            })
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

fn tokens(text: &str) -> (SessionTokens, super::transcript::TranscriptActivity) {
    let mut total = SessionTokens::default();
    let mut activity = super::transcript::TranscriptActivity::default();
    for event in json_lines(text) {
        activity.observe_codex(&event);
        let payload = event.get("payload").unwrap_or(&event);
        if payload.get("type").and_then(Value::as_str) == Some("token_count") {
            if let Some(usage) = payload
                .get("info")
                .and_then(|info| info.get("total_token_usage"))
                .filter(|usage| usage.is_object())
            {
                let input = integer(usage, &["input_tokens"]);
                let output = integer(usage, &["output_tokens"]);
                let cached = integer(usage, &["cached_input_tokens"]);
                total = SessionTokens {
                    input: input.saturating_sub(cached),
                    output,
                    cache_read: cached,
                    reasoning: integer(usage, &["reasoning_output_tokens"]),
                    total: usage
                        .get("total_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or_else(|| input.saturating_add(output)),
                    ..SessionTokens::default()
                };
                continue;
            }
        }
        let usage = event
            .get("usage")
            .or_else(|| {
                event
                    .get("payload")
                    .and_then(|payload| payload.get("usage"))
            })
            .or_else(|| event.get("token_usage"));
        let Some(usage) = usage else {
            continue;
        };
        let current = SessionTokens {
            input: integer(usage, &["input", "input_tokens"]),
            output: integer(usage, &["output", "output_tokens"]),
            reasoning: integer(usage, &["reasoning", "reasoning_tokens"]),
            cache_read: integer(usage, &["cache_read", "cache_read_input_tokens"]),
            cache_write: integer(usage, &["cache_write", "cache_creation_input_tokens"]),
            total: integer(usage, &["total", "total_tokens"]),
            ..SessionTokens::default()
        };
        total.add_assign(&current);
    }
    (total, activity)
}

fn integer(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or_default()
}
