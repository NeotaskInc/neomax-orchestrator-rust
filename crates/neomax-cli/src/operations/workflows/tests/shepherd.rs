use std::path::Path;

use neomax_core::Engine;
use neomax_core::orchestration::registry::OrchestratorRecord;
use neomax_core::runs::{ProbeState, RunRecord, RunStore};
use neomax_core::shepherd::{
    MergePolicy, MergeReadinessInput, ReadyDestination, ShepherdDecision, evaluate_merge_readiness,
};

use super::super::shepherd::{
    decision_json, decision_text, matching_live_orchestrators, persist_pr_url,
};

fn orchestrator_record(session: &str, cwd: &Path, live: bool) -> OrchestratorRecord {
    OrchestratorRecord {
        session: session.into(),
        pid: None,
        engine: Engine::Codex,
        account: Some(2),
        account_dir: ".codex-acct2".into(),
        project: Some("project".into()),
        branch_prefix: Some("feature".into()),
        cwd: cwd.to_path_buf(),
        model: "model".into(),
        reserved: false,
        started: 1,
        last_seen: 1,
        live,
        process_state: ProbeState::Unknown,
        extra: Default::default(),
    }
}

#[test]
fn premerge_lists_other_live_overlapping_orchestrators_only() {
    let temp = tempfile::tempdir().expect("temporary root");
    let workspace = temp.path().join("workspace");
    let repository = workspace.join("repo");
    let worktree = repository.join("worktree");
    let other_repository = workspace.join("other");
    let records = vec![
        orchestrator_record("current", &repository, true),
        orchestrator_record("other", &worktree, true),
        orchestrator_record("dead", &repository, false),
        orchestrator_record("outside", &other_repository, true),
    ];

    let rows = matching_live_orchestrators(&workspace, &repository, Some("current"), &records);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["engine"], "codex");
    assert_eq!(rows[0]["account"], 2);
    assert_eq!(rows[0]["project"], "project");
    assert_eq!(rows[0]["branch_prefix"], "feature");
}

#[test]
fn local_ready_shepherd_result_has_machine_and_human_forms() {
    let decision = evaluate_merge_readiness(
        &MergeReadinessInput::local("feature", "main", "sha", 2),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Ready {
            destination: ReadyDestination::Local,
            ahead: 2,
            ..
        }
    ));
    assert_eq!(decision_json(&decision)["status"], "ready");
    assert!(decision_text(&decision).starts_with("ready (local)"));
}

#[test]
fn expected_head_movement_is_stopped_fail_closed() {
    let decision = evaluate_merge_readiness(
        &MergeReadinessInput::local("feature", "main", "new", 1).expected_sha("old"),
        MergePolicy::default(),
    );
    assert_eq!(decision_json(&decision)["status"], "stopped");
    assert!(decision_text(&decision).contains("HEAD moved"));
}

#[test]
fn pull_request_receipts_are_persisted_for_run_commands() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profiles/claude-1");
    let workdir = temp.path().join("workspace");
    let store = RunStore::new(temp.path());
    let run: RunRecord = serde_json::from_value(serde_json::json!({
        "id": "run-1",
        "engine": "claude",
        "model": "model",
        "prompt": "task",
        "profile": profile,
        "workdir": workdir,
        "status": "done",
        "started": 1
    }))
    .unwrap();
    store.create(&run).unwrap();

    persist_pr_url(&store, "run-1", "https://github.com/example/project/pull/4").unwrap();

    assert_eq!(
        store.load("run-1").unwrap().pr_url.as_deref(),
        Some("https://github.com/example/project/pull/4")
    );
}
