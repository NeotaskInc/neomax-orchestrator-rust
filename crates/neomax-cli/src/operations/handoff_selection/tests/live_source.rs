use std::collections::BTreeMap;

use crate::operations::handoff::options::HandoffOptions;
use crate::operations::handoff::selection::select_live_orchestrator;
use crate::tests::fixture;
use neomax_core::Engine;
use neomax_core::runs::{ProbeState, ProcessProbe};

use super::account;

struct LiveProbe;

impl ProcessProbe for LiveProbe {
    fn pid_alive(&self, _pid: u32) -> bool {
        true
    }

    fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
        false
    }

    fn pid_state(&self, _pid: u32) -> ProbeState {
        ProbeState::Alive
    }
}

#[test]
fn live_registry_selection_preserves_scope_project_branch_and_session() {
    let fixture = fixture();
    let source_profile = fixture.context.paths.home.join(".opencode-acct1");
    let target_profile = fixture.context.paths.home.join(".opencode-acct2");
    std::fs::create_dir_all(&source_profile).unwrap();
    std::fs::create_dir_all(&target_profile).unwrap();
    let store = neomax_core::orchestration::registry::OrchestratorStore::new(
        &fixture.context.paths.orchestrators,
    );
    store
        .register_with_metadata(
            neomax_core::orchestration::registry::OrchestratorRegistration {
                session: "interactive".into(),
                pid: Some(std::process::id()),
                engine: Engine::Opencode,
                account: Some(1),
                account_dir: ".opencode-acct1".into(),
                project: Some("project-a".into()),
                branch_prefix: Some("feature-a".into()),
                cwd: fixture.context.cwd.clone(),
                model: "opencode/big-pickle".into(),
                reserved: false,
                now: fixture.context.now,
            },
            BTreeMap::from([("worker_scope".into(), serde_json::json!("claude,opencode"))]),
        )
        .unwrap();
    let options = HandoffOptions {
        engine: Engine::Opencode,
        source_account: None,
        target_selectors: vec!["2".into()],
        reason: "quota".into(),
        reason_explicit: true,
        cwd: fixture.context.cwd.clone(),
        kickoff: None,
        worker_scope: None,
        model_overrides: BTreeMap::new(),
        environment: Default::default(),
        headless: true,
        check: false,
        dry_run: true,
        json: false,
        run_id: None,
        session: None,
        interactive_only: false,
    };
    let accounts = [
        account(
            Engine::Opencode,
            "1",
            &source_profile.to_string_lossy(),
            99.0,
            0.0,
        ),
        account(
            Engine::Opencode,
            "2",
            &target_profile.to_string_lossy(),
            10.0,
            10.0,
        ),
    ];
    let record = store
        .live(&LiveProbe, fixture.context.now)
        .unwrap()
        .pop()
        .unwrap();
    let (live_options, selected) =
        select_live_orchestrator(&options, &fixture.context, &accounts, &record).unwrap();
    assert_eq!(live_options.engine, Engine::Opencode);
    assert_eq!(live_options.session.as_deref(), Some("interactive"));
    assert_eq!(
        live_options.worker_scope.as_deref(),
        Some("claude,opencode")
    );
    assert!(live_options.interactive_only);
    assert_eq!(
        live_options.environment.values.get("NEOMAX_PROJECT"),
        Some(&"project-a".into())
    );
    assert_eq!(
        live_options.environment.values.get("NEOMAX_BRANCH_PREFIX"),
        Some(&"feature-a".into())
    );
    assert_eq!(
        selected.current_profile,
        std::fs::canonicalize(source_profile).unwrap()
    );
    assert_eq!(selected.source.account, "1");
    assert_eq!(selected.target.unwrap().account.account, "2");
}
