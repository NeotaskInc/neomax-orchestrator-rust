use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;
use std::collections::BTreeMap;

use super::super::{LaunchOptions, LaunchPlan, render, types::LaunchMode};

#[test]
fn solo_mode_is_explicit_and_cannot_become_a_worker_dispatch() {
    let options = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Claude),
        &[
            "--solo".into(),
            "--foreground".into(),
            "--model".into(),
            "fixture/solo-model".into(),
            "solo task".into(),
        ],
    )
    .unwrap();
    assert!(options.solo);
    assert!(!options.worker_dispatch);
    assert_eq!(options.positionals, ["solo task"]);
    assert_eq!(options.model.as_deref(), Some("fixture/solo-model"));
}

#[test]
fn universal_solo_account_shorthand_selects_one_account_without_worker_dispatch() {
    let options = LaunchOptions::parse(
        Launcher::Universal,
        &["--solo".into(), "2".into(), "solo task".into()],
    )
    .unwrap();
    assert!(options.solo);
    assert!(!options.worker_dispatch);
    assert_eq!(options.account.as_deref(), Some("2"));
    assert_eq!(options.routing, "account");
    assert_eq!(options.positionals, ["solo task"]);
}

#[test]
fn solo_mode_rejects_worktree_and_worker_controls() {
    for args in [
        vec!["--solo".into(), "--goal".into(), "verify".into()],
        vec!["--solo".into(), "--base".into(), "main".into()],
        vec!["--solo".into(), "--no-worktree".into()],
        vec!["--solo".into(), "--workers".into(), "codex".into()],
        vec!["--solo".into(), "--prefer".into(), "codex".into()],
        vec!["--solo".into(), "--max-turns".into(), "3".into()],
        vec!["--solo".into(), "--pr".into()],
        vec!["--solo".into(), "--plan".into()],
        vec!["--solo".into(), "--brief".into()],
        vec!["--solo".into(), "--detach".into()],
        vec!["--solo".into(), "--orchestrator".into()],
        vec!["--solo".into(), "-n".into()],
        vec!["--solo".into(), "-t".into(), "10".into()],
        vec!["--solo".into(), "-s".into(), "10".into()],
        vec!["--solo".into(), "--worker-dispatch".into()],
    ] {
        assert!(
            LaunchOptions::parse(Launcher::ProviderOrchestrator(Engine::Claude), &args).is_err(),
            "solo accepted invalid controls: {args:?}"
        );
    }
}

#[test]
fn text_dry_run_reports_read_only_plan_guarantees() {
    let plan = LaunchPlan {
        invocation: "neomax".into(),
        mode: LaunchMode::Dynamic,
        orchestrator: Some("codex".into()),
        worker_engines: vec!["codex".into()],
        routing: "auto".into(),
        account: None,
        operation: None,
        operation_args: Vec::new(),
        initial_task: Some("inspect the checkout".into()),
        goal: None,
        base: None,
        run_id: None,
        tag: None,
        session_id: None,
        max_turns: None,
        priority: None,
        effort: None,
        wall_min: Some(240.0),
        stall_min: None,
        no_failover: false,
        no_worktree: true,
        plan_mode: true,
        open_pull_request: false,
        brief: false,
        ultra: false,
        opus: false,
        resume: false,
        dedicated: false,
        detach: false,
        foreground: true,
        worker_dispatch: true,
        solo: false,
        models: BTreeMap::new(),
        adapters: Vec::new(),
        dry_run: true,
        provider_execution: "disabled".into(),
    };
    let output = render::text(&plan);
    assert!(output.contains("plan_mode = true"));
    assert!(output.contains("current checkout"));
    assert!(output.contains("provider read-only boundary"));
    assert!(output.contains("no provider execution in this dry-run"));
}
