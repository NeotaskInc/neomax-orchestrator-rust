use std::collections::BTreeMap;

use crate::operations::handoff::options::HandoffOptions;
use crate::operations::handoff::selection::{context_time, select_with_environment};
use crate::tests::fixture;
use neomax_core::Engine;
use neomax_core::accounts::AccountSnapshot;
use neomax_core::orchestration::continuation::{
    ContinuationMode, ContinuationRequest, RotationTrigger,
};
use neomax_core::orchestration::handoff::HandoffStore;
use neomax_core::runs::{RunRecord, RunStore};

use super::super::{continue_tracked_run, find_current_run};

#[test]
fn current_run_matching_preserves_work_metadata_for_continuation_request() {
    let fixture = fixture();
    let source_profile = fixture.context.paths.home.join(".opencode");
    let repo = fixture.context.cwd.join("repo");
    let worktree = repo.join(".worktree");
    let runs = RunStore::new(&fixture.context.paths.runs);
    let mut run = RunRecord::new(
        "run-1",
        Engine::Opencode,
        "opencode/big-pickle",
        "continue task",
        &source_profile,
        &fixture.context.cwd,
        fixture.context.now,
    );
    run.cwd = Some(fixture.context.cwd.clone());
    run.repo = Some(repo.clone());
    run.worktree = Some(worktree.clone());
    run.branch = Some("neomax/work".into());
    run.base = Some("main".into());
    run.project = Some("project".into());
    run.session = Some("session-1".into());
    runs.create(&run).unwrap();
    let options = HandoffOptions {
        engine: Engine::Opencode,
        source_account: Some("1".into()),
        target_selectors: vec!["2".into()],
        reason: "quota".into(),
        reason_explicit: true,
        cwd: fixture.context.cwd.clone(),
        kickoff: None,
        worker_scope: Some("claude,opencode".into()),
        model_overrides: BTreeMap::new(),
        environment: Default::default(),
        headless: true,
        check: false,
        dry_run: false,
        json: false,
        run_id: Some("run-1".into()),
        session: Some("session-1".into()),
        interactive_only: false,
    };
    let accounts = vec![
        AccountSnapshot {
            engine: Engine::Opencode,
            account: "1".into(),
            profile: source_profile.clone(),
            binary_available: true,
            authenticated: true,
            rotation_eligible: false,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: None,
            weekly_percent: Some(99.0),
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        },
        AccountSnapshot {
            engine: Engine::Opencode,
            account: "2".into(),
            profile: fixture.context.paths.home.join(".opencode-acct2"),
            binary_available: true,
            authenticated: true,
            rotation_eligible: false,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: None,
            weekly_percent: Some(10.0),
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        },
    ];
    let environment = BTreeMap::from([
        ("NEOMAX_ROLE".into(), "opencode".into()),
        (
            "XDG_DATA_HOME".into(),
            source_profile.to_string_lossy().into_owned(),
        ),
    ]);
    let selection =
        select_with_environment(&options, &fixture.context, &accounts, &environment).unwrap();
    let original = runs.load("run-1").unwrap();
    let request = ContinuationRequest::from_run(
        &original,
        selection.target.as_ref().unwrap().account.clone(),
        RotationTrigger::Manual,
        context_time(&fixture.context),
    );
    assert_eq!(request.repo, Some(repo));
    assert_eq!(request.worktree, Some(worktree));
    assert_eq!(request.branch.as_deref(), Some("neomax/work"));
    assert_eq!(request.session.as_deref(), Some("session-1"));
    let mut interactive_options = options.clone();
    interactive_options.interactive_only = true;
    assert!(
        find_current_run(&runs, &interactive_options, &selection, &fixture.context)
            .unwrap()
            .is_none()
    );
}

#[test]
fn tracked_handoff_updates_only_routing_state_and_keeps_work_metadata() {
    let fixture = fixture();
    let source_profile = fixture.context.paths.home.join(".opencode");
    let repo = fixture.context.cwd.join("repo");
    let worktree = repo.join(".worktree");
    let runs = RunStore::new(&fixture.context.paths.runs);
    let mut run = RunRecord::new(
        "run-2",
        Engine::Opencode,
        "opencode/big-pickle",
        "continue task",
        &source_profile,
        &fixture.context.cwd,
        fixture.context.now,
    );
    run.cwd = Some(fixture.context.cwd.clone());
    run.repo = Some(repo.clone());
    run.worktree = Some(worktree.clone());
    run.branch = Some("neomax/work".into());
    run.base = Some("main".into());
    run.project = Some("project".into());
    run.session = Some("session-2".into());
    runs.create(&run).unwrap();
    let options = HandoffOptions {
        engine: Engine::Opencode,
        source_account: Some("1".into()),
        target_selectors: vec!["2".into()],
        reason: "weekly limit".into(),
        reason_explicit: true,
        cwd: fixture.context.cwd.clone(),
        kickoff: None,
        worker_scope: Some("claude,opencode".into()),
        model_overrides: BTreeMap::new(),
        environment: Default::default(),
        headless: true,
        check: false,
        dry_run: false,
        json: false,
        run_id: Some("run-2".into()),
        session: Some("session-2".into()),
        interactive_only: false,
    };
    let accounts = vec![
        AccountSnapshot {
            engine: Engine::Opencode,
            account: "1".into(),
            profile: source_profile.clone(),
            binary_available: true,
            authenticated: true,
            rotation_eligible: false,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: None,
            weekly_percent: Some(99.0),
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        },
        AccountSnapshot {
            engine: Engine::Opencode,
            account: "2".into(),
            profile: fixture.context.paths.home.join(".opencode-acct2"),
            binary_available: true,
            authenticated: true,
            rotation_eligible: false,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: None,
            weekly_percent: Some(10.0),
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        },
    ];
    let environment = BTreeMap::from([
        ("NEOMAX_ROLE".into(), "opencode".into()),
        (
            "XDG_DATA_HOME".into(),
            source_profile.to_string_lossy().into_owned(),
        ),
    ]);
    let selection =
        select_with_environment(&options, &fixture.context, &accounts, &environment).unwrap();
    let target = selection.target.clone().unwrap().account;
    let original = runs.load("run-2").unwrap();
    let mode = continue_tracked_run(
        &original,
        &options,
        &selection,
        target.clone(),
        &fixture.context,
        &runs,
        RotationTrigger::Manual,
    )
    .unwrap();
    assert_eq!(mode, ContinuationMode::SameProviderHandoff);
    let updated = runs.load("run-2").unwrap();
    assert_eq!(updated.profile, target.profile);
    assert_eq!(updated.repo, Some(repo));
    assert_eq!(updated.worktree, Some(worktree.clone()));
    assert_eq!(updated.branch.as_deref(), Some("neomax/work"));
    assert_eq!(updated.base.as_deref(), Some("main"));
    assert_eq!(updated.project.as_deref(), Some("project"));
    assert_eq!(updated.session, None);
    assert_eq!(updated.session_history[0].session, "session-2");
    let baton = HandoffStore::at_state_dir(&fixture.context.paths.state)
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(baton.extra["run_id"], "run-2");
    assert_eq!(baton.extra["branch"], "neomax/work");
    assert_eq!(
        baton.extra["worktree"].as_str(),
        Some(worktree.to_string_lossy().as_ref())
    );
}
