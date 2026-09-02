use super::safety::{account_label, model_label, session_label, text_label};
use super::types::{AmbientView, OrchestratorView};
use neomax_core::sessions::PortalSnapshot;

pub(super) fn views(snapshot: &PortalSnapshot) -> Vec<AmbientView> {
    snapshot
        .all_records()
        .map(|record| AmbientView {
            id: session_label(&record.id),
            engine: record.engine,
            account: account_label(&record.account),
            model: record.model.clone().map(|value| model_label(&value)),
            project: record.project.clone().map(|value| text_label(&value)),
            branch: record.branch.clone().map(|value| text_label(&value)),
            label: record.label.clone().map(|value| text_label(&value)),
            kind: record.kind,
            parent_id: record.parent_id.as_deref().map(session_label),
            active: record.active,
            working: record.working,
            started: record.started,
            last_active: record.last_active,
            input: record.tokens.input,
            output: record.tokens.output,
            reasoning: record.tokens.reasoning,
            cache_read: record.tokens.cache_read,
            cache_write: record.tokens.cache_write,
            requests: record.requests,
            completions: record.completions,
            errors: record.errors,
            rate_limits: record.rate_limits,
            tool_calls: record.tool_calls,
            tool_errors: record.tool_errors,
        })
        .collect()
}

pub(super) fn orchestrator(
    record: neomax_core::orchestration::registry::OrchestratorRecord,
) -> OrchestratorView {
    OrchestratorView {
        session: session_label(&record.session),
        pid: record.pid,
        engine: record.engine,
        account: account_label(&record.account_dir),
        project: record.project.map(|value| text_label(&value)),
        model: model_label(&record.model),
        reserved: record.reserved,
        started: record.started,
        last_seen: record.last_seen,
        live: record.live,
    }
}
