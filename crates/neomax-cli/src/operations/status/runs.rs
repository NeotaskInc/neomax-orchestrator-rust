use neomax_core::runs::{RunRecord, RunStatus, SystemProcessProbe, effective_status};
use serde_json::Value;

use super::safety::{account_label, model_label, session_label, tag_label, text_label};
use super::types::{RunView, SessionView, SubagentView};

pub(super) fn view_run(run: &RunRecord, probe: &SystemProcessProbe) -> RunView {
    let id = session_label(&run.id);
    RunView {
        id,
        engine: run.engine,
        model: model_label(&run.model),
        status: effective_status(run, probe).as_str().to_owned(),
        account: account_label(
            run.profile
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown"),
        ),
        session: run.session.as_deref().map(session_label),
        started: run.started,
        ended: run.ended,
        worker_pid: run.worker_pid,
        supervisor_pid: run.supervisor_pid,
        attempt: run.attempt,
        project: run.project.as_deref().map(text_label),
        branch: run.branch.as_deref().map(text_label),
        tag: run.tag.as_deref().map(tag_label),
        worktree_state: run.worktree_state.as_deref().map(text_label),
        child_count: live_child_count(run, probe),
        has_error: run.error_detail.is_some(),
        acknowledged: run.is_acknowledged(),
    }
}

pub(super) fn view_session(run: &RunRecord, probe: &SystemProcessProbe) -> Option<SessionView> {
    let session = run.session.as_ref()?.trim();
    if session.is_empty() {
        return None;
    }
    let status = effective_status(run, probe);
    if !matches!(status, RunStatus::Running | RunStatus::Orphaned) {
        return None;
    }
    Some(SessionView {
        id: session_label(session),
        run_id: session_label(&run.id),
        engine: run.engine,
        account: account_label(
            run.profile
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown"),
        ),
        model: run.model.clone(),
        status: status.as_str().to_owned(),
        started: run.started,
        worker_pid: run.worker_pid,
        child_count: live_child_count(run, probe),
    })
}

pub(super) fn view_subagents(run: &RunRecord, probe: &SystemProcessProbe) -> Vec<SubagentView> {
    if !matches!(
        effective_status(run, probe),
        RunStatus::Running | RunStatus::Orphaned
    ) {
        return Vec::new();
    }
    run.children
        .iter()
        .enumerate()
        .filter_map(|(index, child)| {
            let run_id = session_label(&run.id);
            let object = child.as_object()?;
            let status = object
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("running")
                .to_owned();
            if status != "running" {
                return None;
            }
            let id = object
                .get("id")
                .or_else(|| object.get("agent"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(session_label)
                .unwrap_or_else(|| format!("{run_id}-subagent-{}", index + 1));
            let label = safe_child_string(object, &["label", "name", "agent"]);
            let model = safe_child_string(object, &["model"]).map(|value| model_label(&value));
            Some(SubagentView {
                id,
                run_id,
                engine: run.engine,
                account: account_label(
                    run.profile
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("unknown"),
                ),
                status,
                label,
                model,
            })
        })
        .collect()
}

pub(super) fn live_child_count(run: &RunRecord, probe: &SystemProcessProbe) -> usize {
    if !matches!(
        effective_status(run, probe),
        RunStatus::Running | RunStatus::Orphaned
    ) {
        return 0;
    }
    run.children
        .iter()
        .filter(|child| {
            child
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("running")
                == "running"
                && child.get("kind").and_then(Value::as_str) != Some("step")
        })
        .count()
}

fn safe_child_string(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(|value| value.chars().take(160).collect())
}
