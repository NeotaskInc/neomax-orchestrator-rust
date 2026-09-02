use std::path::Path;

use super::types::ArtifactKind;

pub(super) fn matches_kind(path: &Path, profile: &Path, kind: ArtifactKind) -> bool {
    let relative = path.strip_prefix(profile).unwrap_or(path);
    let components = relative.components().collect::<Vec<_>>();
    let under = |name: &str| {
        components
            .iter()
            .any(|component| component.as_os_str() == name)
    };
    match kind {
        ArtifactKind::ClaudeMain => {
            components
                .first()
                .is_some_and(|component| component.as_os_str() == "projects")
                && !under("subagents")
                && path.extension().and_then(|value| value.to_str()) == Some("jsonl")
        }
        ArtifactKind::ClaudeSubagent => {
            under("subagents")
                && path.file_name().and_then(|value| value.to_str()) != Some("journal.jsonl")
                && path.extension().and_then(|value| value.to_str()) == Some("jsonl")
        }
        ArtifactKind::CodexRollout => {
            under("sessions")
                && path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
        }
        ArtifactKind::KimiState => {
            under("sessions")
                && path.file_name().and_then(|value| value.to_str()) == Some("state.json")
        }
        ArtifactKind::KimiWire => {
            under("sessions")
                && path.file_name().and_then(|value| value.to_str()) == Some("wire.jsonl")
        }
        ArtifactKind::GrokSummary => {
            under("sessions")
                && path.file_name().and_then(|value| value.to_str()) == Some("summary.json")
        }
        ArtifactKind::GrokUpdates => {
            under("sessions")
                && path.file_name().and_then(|value| value.to_str()) == Some("updates.jsonl")
        }
        ArtifactKind::OpenCodeDatabase => {
            path.file_name().and_then(|value| value.to_str()) == Some("opencode.db")
        }
    }
}
