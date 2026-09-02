use std::collections::BTreeMap;

use crate::operations::handoff::options::HandoffOptions;
use crate::operations::handoff::selection::HandoffSelection;
use crate::tests::fixture;
use neomax_core::Engine;
use neomax_core::accounts::AccountSnapshot;
use neomax_core::orchestration::continuation::{ContinuationMode, RotationTrigger};
use neomax_core::orchestration::handoff::{
    HandoffAdvice, HandoffCheck, TargetSelection, TargetTier,
};
use neomax_core::runs::{RunRecord, RunStore};

use super::super::continue_tracked_run;

#[test]
fn quota_tick_reaches_cross_provider_tracked_handoff_but_manual_does_not() {
    let fixture = fixture();
    let runs = RunStore::new(&fixture.context.paths.runs);
    let source_profile = fixture.context.paths.home.join(".claude");
    let target_profile = fixture.context.paths.home.join(".opencode-acct2");
    let mut run = RunRecord::new(
        "run-cross-tick",
        Engine::Claude,
        "claude-sonnet",
        "continue task",
        source_profile.clone(),
        &fixture.context.cwd,
        fixture.context.now,
    );
    run.status = neomax_core::runs::RunStatus::Limit;
    run.session = Some("session-cross".into());
    runs.create(&run).unwrap();

    let source = AccountSnapshot {
        engine: Engine::Claude,
        account: "1".into(),
        profile: source_profile.clone(),
        binary_available: true,
        authenticated: true,
        rotation_eligible: false,
        paused: false,
        reserved: false,
        live_workers: 0,
        five_hour_percent: Some(99.0),
        weekly_percent: Some(10.0),
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    };
    let target = AccountSnapshot {
        engine: Engine::Opencode,
        account: "2".into(),
        profile: target_profile,
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
    };
    let selection = HandoffSelection {
        engine: Engine::Claude,
        current_profile: source_profile,
        source,
        target: Some(TargetSelection {
            account: target.clone(),
            tier: TargetTier::HardHeadroom,
        }),
        check: HandoffCheck {
            engine: Engine::Claude,
            account: "1".into(),
            five_hour: 99.0,
            seven_day: 10.0,
            advice: HandoffAdvice {
                advised: true,
                reason: "five-hour hard wall".into(),
            },
            target_account: Some("2".into()),
            target_weekly_resets: None,
            target_email: None,
        },
    };
    let options = HandoffOptions {
        engine: Engine::Claude,
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
        run_id: Some("run-cross-tick".into()),
        session: Some("session-cross".into()),
        interactive_only: false,
    };

    let original = runs.load("run-cross-tick").unwrap();
    let manual = continue_tracked_run(
        &original,
        &options,
        &selection,
        target.clone(),
        &fixture.context,
        &runs,
        RotationTrigger::Manual,
    )
    .unwrap_err();
    assert!(manual.to_string().contains("only after a quota event"));

    let mode = continue_tracked_run(
        &original,
        &options,
        &selection,
        target,
        &fixture.context,
        &runs,
        RotationTrigger::Tick,
    )
    .unwrap();
    assert_eq!(mode, ContinuationMode::CrossProviderHandoff);
    let updated = runs.load("run-cross-tick").unwrap();
    assert_eq!(updated.engine, Engine::Opencode);
    assert_eq!(updated.workdir, fixture.context.cwd);
    assert!(updated.session.is_none());
}
