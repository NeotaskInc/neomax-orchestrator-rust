use crate::runs::RunRecord;

use super::OrchestratorRecord;

pub fn run_owner(run: &RunRecord) -> Option<&str> {
    run.orch_session.as_deref()
}

pub fn owned_by_other_live_orchestrator(
    run: &RunRecord,
    current_session: Option<&str>,
    orchestrators: &[OrchestratorRecord],
) -> bool {
    let Some(current) = current_session else {
        return false;
    };
    let Some(owner) = run_owner(run) else {
        return false;
    };
    owner != current
        && orchestrators
            .iter()
            .any(|record| record.live && record.session == owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_other_live_sessions_without_scoping_bare_cli_calls() {
        let run: RunRecord = serde_json::from_value(serde_json::json!({
            "id":"run", "status":"running", "started":1, "orch_session":"owner"
        }))
        .unwrap();
        let orchestrator: OrchestratorRecord = serde_json::from_value(serde_json::json!({
            "session":"owner", "engine":"claude", "last_seen":1, "live":true
        }))
        .unwrap();
        assert!(owned_by_other_live_orchestrator(
            &run,
            Some("caller"),
            std::slice::from_ref(&orchestrator)
        ));
        assert!(!owned_by_other_live_orchestrator(
            &run,
            Some("owner"),
            std::slice::from_ref(&orchestrator)
        ));
        assert!(!owned_by_other_live_orchestrator(
            &run,
            None,
            &[orchestrator]
        ));
    }
}
