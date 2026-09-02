use std::collections::BTreeMap;

use crate::operations::handoff::options::HandoffOptions;
use crate::operations::handoff::selection::select_with_environment;
use crate::tests::fixture;
use neomax_core::Engine;
use neomax_core::orchestration::handoff::HandoffCheck;

use super::account;

#[test]
fn check_tolerates_no_target_but_preserves_exit_advice() {
    let fixture = fixture();
    let source_profile = fixture.context.paths.home.join(".kimi-code");
    let options = HandoffOptions {
        engine: Engine::Kimi,
        source_account: Some("1".into()),
        target_selectors: Vec::new(),
        reason: "quota".into(),
        reason_explicit: true,
        cwd: fixture.context.cwd.clone(),
        kickoff: None,
        worker_scope: None,
        model_overrides: BTreeMap::new(),
        environment: Default::default(),
        headless: true,
        check: true,
        dry_run: false,
        json: false,
        run_id: None,
        session: None,
        interactive_only: false,
    };
    let accounts = [account(
        Engine::Kimi,
        "1",
        &source_profile.to_string_lossy(),
        0.0,
        99.0,
    )];
    let environment = BTreeMap::from([
        ("NEOMAX_ROLE".into(), "kimi".into()),
        (
            "KIMI_CODE_HOME".into(),
            source_profile.to_string_lossy().into_owned(),
        ),
    ]);
    let selected =
        select_with_environment(&options, &fixture.context, &accounts, &environment).unwrap();
    assert!(selected.target.is_none());
    assert_eq!(selected.check.exit_code(), HandoffCheck::ROTATE_EXIT);
}

#[test]
fn explicit_selector_rejects_wrong_provider() {
    let fixture = fixture();
    let wrong_provider_profile = fixture.context.paths.home.join(".claude-acct2");
    let mut options = HandoffOptions {
        engine: Engine::Grok,
        source_account: None,
        target_selectors: vec!["2".into()],
        reason: "manual".into(),
        reason_explicit: true,
        cwd: fixture.context.cwd.clone(),
        kickoff: None,
        worker_scope: None,
        model_overrides: BTreeMap::new(),
        environment: Default::default(),
        headless: true,
        check: false,
        dry_run: false,
        json: false,
        run_id: None,
        session: None,
        interactive_only: false,
    };
    let accounts = [account(
        Engine::Claude,
        "2",
        wrong_provider_profile.to_string_lossy().as_ref(),
        1.0,
        1.0,
    )];
    let environment = BTreeMap::from([(String::from("NEOMAX_ROLE"), String::from("grok"))]);
    assert!(select_with_environment(&options, &fixture.context, &accounts, &environment).is_err());
    options.check = true;
    let selected =
        select_with_environment(&options, &fixture.context, &accounts, &environment).unwrap();
    assert!(selected.target.is_none());
}
