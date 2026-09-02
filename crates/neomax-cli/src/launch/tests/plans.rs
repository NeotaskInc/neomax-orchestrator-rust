use neomax_core::Engine;
use neomax_core::agent_tools::NEOMAX_BIN_ENV;
use neomax_core::orchestration::commands::Launcher;

use crate::tests::fixture;

use super::super::{LaunchOptions, build_plan, types::LaunchMode};

#[test]
fn universal_mode_is_dynamic_and_captures_initial_task_verbatim() {
    let fixture = fixture();
    let options = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--dry-run".into(),
            "--engine".into(),
            "opencode".into(),
            "ship".into(),
            "the".into(),
            "feature".into(),
        ],
    )
    .unwrap();
    let plan = build_plan(Launcher::Universal, options, &fixture.context).unwrap();
    assert_eq!(plan.mode, LaunchMode::Dynamic);
    assert_eq!(plan.orchestrator.as_deref(), Some("opencode"));
    assert_eq!(plan.routing, "default");
    assert_eq!(plan.initial_task.as_deref(), Some("ship the feature"));
    assert_eq!(plan.worker_engines.len(), 5);
    let opencode = plan
        .adapters
        .iter()
        .find(|adapter| adapter.provider == "OpenCode")
        .expect("OpenCode adapter");
    assert_eq!(
        opencode.environment.variables.get(NEOMAX_BIN_ENV),
        Some(&"core-managed".to_owned())
    );
    assert_eq!(opencode.environment.role, "orchestrator");
    assert_eq!(opencode.environment.policy, "orchestrator");
}

#[test]
fn every_provider_orchestrator_accepts_a_different_worker_provider() {
    let pairs = [
        (Engine::Claude, Engine::Kimi),
        (Engine::Codex, Engine::Grok),
        (Engine::Opencode, Engine::Claude),
        (Engine::Kimi, Engine::Codex),
        (Engine::Grok, Engine::Opencode),
    ];
    for (orchestrator, worker) in pairs {
        let fixture = fixture();
        let options = LaunchOptions::parse(
            Launcher::ProviderOrchestrator(orchestrator),
            &[
                "--dry-run".into(),
                "--workers".into(),
                worker.to_string(),
                "task".into(),
            ],
        )
        .unwrap();
        let plan = build_plan(
            Launcher::ProviderOrchestrator(orchestrator),
            options,
            &fixture.context,
        )
        .unwrap();
        assert_eq!(plan.orchestrator.as_deref(), Some(orchestrator.as_str()));
        assert_eq!(plan.worker_engines, vec![worker.to_string()]);
        assert!(plan.adapters.iter().any(|adapter| {
            adapter.role == "orchestrator" && adapter.environment.role == "orchestrator"
        }));
        assert!(plan.adapters.iter().any(|adapter| {
            adapter.role == "worker-pool" && adapter.environment.role == "worker-pool"
        }));
    }
}

#[test]
fn universal_mode_preserves_an_explicit_mixed_worker_scope() {
    let fixture = fixture();
    let options = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--dry-run".into(),
            "--engine=opencode".into(),
            "--workers=kimi+grok".into(),
        ],
    )
    .unwrap();
    let plan = build_plan(Launcher::Universal, options, &fixture.context).unwrap();
    assert_eq!(plan.orchestrator.as_deref(), Some("opencode"));
    assert_eq!(plan.worker_engines, ["kimi", "grok"]);
    assert!(
        plan.adapters
            .iter()
            .any(|adapter| { adapter.provider == "OpenCode" && adapter.role == "orchestrator" })
    );
}

#[test]
fn provider_launcher_is_pinned_and_rejects_a_conflicting_engine() {
    let fixture = fixture();
    let options = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Claude),
        &["--dry-run".into(), "--engine=codex".into()],
    )
    .unwrap();
    let error = build_plan(
        Launcher::ProviderOrchestrator(Engine::Claude),
        options,
        &fixture.context,
    )
    .expect_err("conflicting engine should fail");
    assert!(error.to_string().contains("pinned"));
}

#[test]
fn account_helper_plan_preserves_operation_and_account_without_execution() {
    let fixture = fixture();
    let options = LaunchOptions::parse(
        Launcher::AccountHelper(Engine::Codex),
        &["--dry-run".into(), "login".into(), "2".into()],
    )
    .unwrap();
    let plan = build_plan(
        Launcher::AccountHelper(Engine::Codex),
        options,
        &fixture.context,
    )
    .unwrap();
    assert_eq!(plan.mode, LaunchMode::AccountHelper);
    assert_eq!(plan.operation.as_deref(), Some("login"));
    assert_eq!(plan.account.as_deref(), Some("2"));
    assert_eq!(plan.provider_execution, "disabled");
}

#[test]
fn launch_controls_are_preserved_in_the_plan_instead_of_discarded() {
    let fixture = fixture();
    let options = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--dry-run".into(),
            "--engine=opencode".into(),
            "--workers= kimi + grok ".into(),
            "--prefer".into(),
            "grok+opencode".into(),
            "--account=2".into(),
            "--orchestrator".into(),
            "--goal=tests pass".into(),
            "--base".into(),
            "src".into(),
            "--session-id=session-1".into(),
            "--resume".into(),
            "--max-turns=7".into(),
            "task".into(),
        ],
    )
    .unwrap();
    let plan = build_plan(Launcher::Universal, options, &fixture.context).unwrap();
    assert_eq!(plan.priority.as_deref(), Some("grok+opencode"));
    assert_eq!(plan.account.as_deref(), Some("2"));
    assert!(plan.dedicated);
    assert!(plan.resume);
    assert_eq!(plan.goal.as_deref(), Some("tests pass"));
    assert_eq!(plan.base.as_deref(), Some("src"));
    assert_eq!(plan.session_id.as_deref(), Some("session-1"));
    assert_eq!(plan.max_turns, Some(7));
}

#[test]
fn worker_run_id_and_tag_are_visible_in_dry_run_plans() {
    let fixture = fixture();
    let options = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--dry-run".into(),
            "--worker-dispatch".into(),
            "--engine=codex".into(),
            "--run-id=PLAN-p1".into(),
            "--tag=plan=PLAN".into(),
            "worker task".into(),
        ],
    )
    .unwrap();
    let plan = build_plan(Launcher::Universal, options, &fixture.context).unwrap();
    assert_eq!(plan.run_id.as_deref(), Some("PLAN-p1"));
    assert_eq!(plan.tag.as_deref(), Some("plan=PLAN"));
    let rendered = super::super::render::text(&plan);
    assert!(rendered.contains("run_id = PLAN-p1"));
    assert!(rendered.contains("tag = plan=PLAN"));
}
