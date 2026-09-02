use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{Engine, Result};

use super::activity::{classify_activity, ActivityInput, ActivityState};
use super::artifacts::{Artifact, ArtifactKind, ArtifactSource};
use super::filters::{apply_context, DiscoveryContext};
use super::headers::timestamp_epoch;
use super::subagents;
use super::types::{SessionKind, SessionRecord};

mod wire;

pub fn discover<S: ArtifactSource>(
    source: &S,
    profile: &Path,
    account: &str,
    context: &DiscoveryContext,
    cutoff: i64,
) -> Result<Vec<SessionRecord>> {
    let states = source.discover(profile, ArtifactKind::KimiState, cutoff)?;
    let wires = source.discover(profile, ArtifactKind::KimiWire, cutoff)?;
    let mut by_session = BTreeMap::<PathBuf, Vec<Artifact>>::new();
    for wire in wires {
        if let Some(session_dir) = session_dir_for_wire(&wire.path) {
            by_session
                .entry(session_dir.to_path_buf())
                .or_default()
                .push(wire);
        }
    }
    let mut rows = Vec::new();
    for state in states {
        rows.extend(parse_state(
            &state,
            by_session.get(state.path.parent().unwrap_or(profile)),
            account,
            context,
        ));
    }
    rows.sort_by_key(|record| std::cmp::Reverse(record.last_active.unwrap_or_default()));
    Ok(rows)
}

fn session_dir_for_wire(path: &Path) -> Option<&Path> {
    let agent_dir = path.parent()?;
    let agents_dir = agent_dir.parent()?;
    (agents_dir.file_name().and_then(|value| value.to_str()) == Some("agents"))
        .then(|| agents_dir.parent())
        .flatten()
}

pub fn parse_state(
    state_artifact: &Artifact,
    wires: Option<&Vec<Artifact>>,
    account: &str,
    context: &DiscoveryContext,
) -> Vec<SessionRecord> {
    let state = serde_json::from_slice::<Value>(&state_artifact.bytes).unwrap_or(Value::Null);
    let session_dir = state_artifact.path.parent().unwrap_or(Path::new("."));
    let id = state
        .get("sessionId")
        .or_else(|| state.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            session_dir
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("session")
                .into()
        });
    let cwd = state
        .get("workDir")
        .or_else(|| state.get("cwd"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let agents = state
        .get("agents")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let wire_map = wires
        .into_iter()
        .flatten()
        .filter_map(|wire| {
            let agent = wire
                .path
                .parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())?;
            Some((agent.to_string(), wire))
        })
        .collect::<BTreeMap<_, _>>();
    let mut main_stats = wire::WireStats::default();
    let mut children = Vec::new();
    for (agent_id, meta) in &agents {
        let stats = wire_map
            .get(agent_id)
            .map(|artifact| wire::stats(artifact))
            .unwrap_or_default();
        main_stats.merge(&stats);
        if meta.get("type").and_then(Value::as_str) == Some("main") || agent_id == "main" {
            continue;
        }
        let mut child = SessionRecord::with_identity(agent_id, Engine::Kimi, account);
        child.kind = SessionKind::NativeSubagent;
        child.parent_id = meta
            .get("parentAgentId")
            .and_then(Value::as_str)
            .map(|_| id.clone())
            .or_else(|| Some(id.clone()));
        child.cwd = cwd.clone();
        child.model = stats.model.clone();
        child.label = Some(agent_id.clone());
        child.extra = meta.as_object().map_or_else(BTreeMap::new, |object| {
            super::artifacts::flatten_extra(object, &["type", "parentAgentId", "homedir"])
        });
        child.started = state.get("createdAt").and_then(timestamp_epoch);
        child.last_active = nonzero(stats.last_active).or(Some(state_artifact.modified));
        child.active = classify_activity(ActivityInput {
            now: context.now,
            last_modified: child.last_active.unwrap_or_default(),
            active_window: context.active_window,
            progress: stats.progress,
            ..ActivityInput::default()
        }) == ActivityState::Active;
        child.working = child.active;
        child.done = !child.active;
        child.tokens = stats.tokens;
        child.requests = stats.requests;
        child.completions = stats.completions;
        child.errors = stats.errors;
        child.rate_limits = stats.rate_limits;
        child.tool_calls = stats.tool_calls;
        child.tool_errors = stats.tool_errors;
        child.files = stats.files.into_values().collect();
        if apply_context(&mut child, context).unwrap_or(false) {
            children.push(child);
        }
    }
    let mut main = SessionRecord::with_identity(id, Engine::Kimi, account);
    main.cwd = cwd;
    main.model = main_stats
        .model
        .or_else(|| Some(crate::providers::catalog::KIMI_DEFAULT_MODEL.into()));
    main.label = state
        .get("title")
        .or_else(|| state.get("lastPrompt"))
        .and_then(Value::as_str)
        .map(Into::into);
    main.started = state.get("createdAt").and_then(timestamp_epoch);
    main.last_active = nonzero(main_stats.last_active).or(Some(state_artifact.modified));
    main.active = classify_activity(ActivityInput {
        now: context.now,
        last_modified: main.last_active.unwrap_or_default(),
        active_window: context.active_window,
        progress: main_stats.progress,
        ..ActivityInput::default()
    }) == ActivityState::Active;
    main.working = main.active;
    main.done = !main.active;
    main.tokens = main_stats.tokens;
    main.requests = main_stats.requests;
    main.completions = main_stats.completions;
    main.errors = main_stats.errors;
    main.rate_limits = main_stats.rate_limits;
    main.tool_calls = main_stats.tool_calls;
    main.tool_errors = main_stats.tool_errors;
    main.files = main_stats.files.into_values().collect();
    main.extra = state.as_object().map_or_else(BTreeMap::new, |object| {
        super::artifacts::flatten_extra(
            object,
            &[
                "sessionId",
                "id",
                "workDir",
                "cwd",
                "title",
                "lastPrompt",
                "createdAt",
                "agents",
            ],
        )
    });
    if !apply_context(&mut main, context).unwrap_or(false) {
        return Vec::new();
    }
    let mut rows = vec![main];
    rows.extend(children);
    rows
}

fn nonzero(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

pub fn normalize_children(
    parent: &mut SessionRecord,
    children: impl IntoIterator<Item = SessionRecord>,
) {
    for child in children {
        parent
            .children
            .push(subagents::normalize_child(child, parent));
    }
}
