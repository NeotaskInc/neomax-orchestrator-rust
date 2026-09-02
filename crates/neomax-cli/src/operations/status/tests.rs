use std::fs;

use neomax_core::orchestration::registry::{OrchestratorRegistration, OrchestratorStore};
use neomax_core::runs::{RunRecord, RunStatus, RunStore};

use super::snapshot::build_report;
use crate::tests::fixture;

#[test]
fn status_uses_safe_views_for_empty_fleet() {
    let fixture = fixture();
    let report = build_report(&fixture.context).unwrap();
    assert_eq!(report.engines.len(), 5);
    assert!(report.accounts.is_empty());
    assert!(report.sessions.is_empty());
    assert!(report.subagents.is_empty());
    let encoded = serde_json::to_string(&report).unwrap();
    assert!(!encoded.contains(fixture.context.paths.home.to_string_lossy().as_ref()));
    assert!(!encoded.contains(fixture.context.paths.state.to_string_lossy().as_ref()));
}

#[test]
fn status_reports_live_sessions_and_native_subagents_from_the_run_ledger() {
    let fixture = fixture();
    let profile = fixture.context.cwd.join("account-2");
    fs::create_dir_all(&profile).unwrap();
    let mut run = RunRecord::new(
        "run-1",
        neomax_core::Engine::Codex,
        "gpt-5.6-sol",
        "private prompt",
        &profile,
        &fixture.context.cwd,
        fixture.context.now,
    );
    run.status = RunStatus::Orphaned;
    run.session = Some("session/with secret-looking chars".into());
    run.children = vec![serde_json::json!({
        "id": "agent-1",
        "status": "running",
        "kind": "agent",
        "label": "worker"
    })];
    RunStore::new(&fixture.context.paths.runs)
        .create(&run)
        .unwrap();

    let report = build_report(&fixture.context).unwrap();
    assert_eq!(report.sessions.len(), 1);
    assert_eq!(report.subagents.len(), 1);
    assert_eq!(report.runs[0].child_count, 1);
    assert_eq!(report.sessions[0].id, "withsecret-lookingchars");
    assert_eq!(report.subagents[0].id, "agent-1");
    assert_eq!(report.summary.live_sessions, 1);
    let encoded = serde_json::to_string(&report).unwrap();
    assert!(!encoded.contains("private prompt"));
}

#[test]
fn status_redacts_path_shaped_identity_fields() {
    let fixture = fixture();
    let private_root = fixture.context.paths.home.join("secret-root");
    let account_dir = private_root.join(".codex-acct2");
    let project = private_root.join("project");
    OrchestratorStore::new(&fixture.context.paths.orchestrators)
        .register(OrchestratorRegistration {
            session: "session-1".into(),
            pid: None,
            engine: neomax_core::Engine::Codex,
            account: Some(2),
            account_dir: account_dir.to_string_lossy().into_owned(),
            project: Some(project.to_string_lossy().into_owned()),
            branch_prefix: None,
            cwd: project,
            model: "gpt-5.6-sol".into(),
            reserved: false,
            now: fixture.context.now,
        })
        .unwrap();

    let report = build_report(&fixture.context).unwrap();
    let encoded = serde_json::to_string(&report).unwrap();
    assert!(!encoded.contains(private_root.to_string_lossy().as_ref()));
    assert_eq!(report.orchestrators[0].account, ".codex-acct2");
    assert_eq!(report.orchestrators[0].project.as_deref(), Some("project"));
}
