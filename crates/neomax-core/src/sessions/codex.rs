use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{Engine, Result};

use super::activity::ActivityState;
use super::artifacts::{json_lines, ArtifactKind, ArtifactSource};
use super::filters::{apply_context, DiscoveryContext};
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
    let mut rows = source
        .discover(profile, ArtifactKind::CodexRollout, cutoff)?
        .into_iter()
        .filter_map(|artifact| parse_rollout(&artifact, account, context))
        .collect::<Vec<_>>();
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
    record.kind = SessionKind::Main;
    record.model = model(&head);
    record.cwd = meta.cwd.map(PathBuf::from);
    record.branch = meta.branch;
    record.label = meta.label;
    record.started = meta.started.or(Some(artifact.modified));
    record.last_active = Some(artifact.modified);
    record.active = live == ActivityState::Active;
    record.working = working == ActivityState::Active;
    record.done = live == ActivityState::Stopped;
    record.tokens = tokens(&artifact.text());
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

fn tokens(text: &str) -> SessionTokens {
    let mut total = SessionTokens::default();
    for event in json_lines(text) {
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
    total
}

fn integer(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or_default()
}
