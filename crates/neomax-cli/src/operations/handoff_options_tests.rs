use super::*;
use crate::tests::fixture;
use neomax_core::orchestration::registry::OrchestratorRecord;
use neomax_core::runs::ProbeState;

#[test]
fn parses_repeated_target_aliases_and_prompt() {
    let fixture = fixture();
    let options = parse(
        Launcher::ProviderOrchestrator(Engine::Opencode),
        &fixture.context,
        &[
            "--to".into(),
            "acct2".into(),
            "--target-account=3".into(),
            "--prompt".into(),
            "resume now".into(),
            "4".into(),
        ],
    )
    .unwrap();
    assert_eq!(options.engine, Engine::Opencode);
    assert_eq!(options.target_selectors, ["2", "3", "4"]);
    assert_eq!(options.kickoff.as_deref(), Some("resume now"));
}

#[test]
fn pinned_launcher_wins_over_role_inference() {
    let fixture = fixture();
    let options = parse(
        Launcher::ProviderOrchestrator(Engine::Kimi),
        &fixture.context,
        &[],
    )
    .unwrap();
    assert_eq!(options.engine, Engine::Kimi);
}

#[test]
fn live_record_context_overrides_shell_defaults_without_dropping_models() {
    let fixture = fixture();
    let options = parse(
        Launcher::ProviderOrchestrator(Engine::Opencode),
        &fixture.context,
        &["--opencode-model", "registry/target"].map(String::from),
    )
    .unwrap();
    let record = OrchestratorRecord {
        session: "session-7".into(),
        pid: Some(7),
        engine: Engine::Opencode,
        account: Some(2),
        account_dir: ".opencode-acct2".into(),
        project: None,
        branch_prefix: None,
        cwd: std::path::PathBuf::from("/workspace/project"),
        model: "record/model".into(),
        reserved: false,
        started: 1,
        last_seen: 2,
        live: true,
        process_state: ProbeState::Alive,
        extra: BTreeMap::from([
            ("worker_scope".into(), "claude,opencode".into()),
            ("project".into(), "project".into()),
            ("branch_prefix".into(), "feature".into()),
        ]),
    };
    let live = options.for_live_orchestrator(&record, None);
    assert_eq!(live.session.as_deref(), Some("session-7"));
    assert_eq!(live.source_account.as_deref(), Some("2"));
    assert_eq!(live.cwd, PathBuf::from("/workspace/project"));
    assert_eq!(live.worker_scope.as_deref(), Some("claude,opencode"));
    assert_eq!(
        live.environment.values.get("NEOMAX_PROJECT"),
        Some(&"project".into())
    );
    assert_eq!(
        live.environment.values.get("NEOMAX_BRANCH_PREFIX"),
        Some(&"feature".into())
    );
    assert_eq!(live.model_overrides[&Engine::Opencode], "registry/target");
    assert!(live.interactive_only);
}

#[test]
fn kimi_live_handoff_starts_the_target_profile_from_its_agent_file() {
    let fixture = fixture();
    let options = parse(
        Launcher::ProviderOrchestrator(Engine::Kimi),
        &fixture.context,
        &[],
    )
    .unwrap();
    let record = OrchestratorRecord {
        session: "kimi-session-7".into(),
        pid: Some(7),
        engine: Engine::Kimi,
        account: Some(1),
        account_dir: ".kimi-code".into(),
        project: None,
        branch_prefix: None,
        cwd: fixture.context.cwd.clone(),
        model: "kimi-code/k3".into(),
        reserved: false,
        started: 1,
        last_seen: 2,
        live: true,
        process_state: ProbeState::Alive,
        extra: BTreeMap::new(),
    };
    let live = options.for_live_orchestrator(&record, None);
    let launch = live.launch_options("1", "2");
    assert!(launch.session_id.is_none());
    assert!(!launch.resume);
}
