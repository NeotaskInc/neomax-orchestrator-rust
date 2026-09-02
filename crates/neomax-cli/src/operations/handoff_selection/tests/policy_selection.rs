use std::collections::BTreeMap;

use crate::operations::handoff::options::HandoffOptions;
use crate::operations::handoff::selection::select_with_environment;
use crate::tests::fixture;
use neomax_core::Engine;

use super::account;

#[test]
fn chooses_same_provider_target_and_uses_five_hour_first() {
    let fixture = fixture();
    let source_profile = fixture.context.paths.home.join(".claude");
    let second_profile = fixture.context.paths.home.join(".claude-acct2");
    let third_profile = fixture.context.paths.home.join(".claude-acct3");
    let codex_profile = fixture.context.paths.home.join(".codex");
    let options = HandoffOptions {
        engine: Engine::Claude,
        source_account: None,
        target_selectors: Vec::new(),
        reason: "quota".into(),
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
    let accounts = [
        account(
            Engine::Claude,
            "1",
            &source_profile.to_string_lossy(),
            99.0,
            10.0,
        ),
        account(
            Engine::Claude,
            "2",
            &second_profile.to_string_lossy(),
            50.0,
            90.0,
        ),
        account(
            Engine::Claude,
            "3",
            &third_profile.to_string_lossy(),
            20.0,
            10.0,
        ),
        account(
            Engine::Codex,
            "1",
            &codex_profile.to_string_lossy(),
            0.0,
            0.0,
        ),
    ];
    let environment = BTreeMap::from([
        ("NEOMAX_ROLE".into(), "claude".into()),
        (
            "CLAUDE_CONFIG_DIR".into(),
            source_profile.to_string_lossy().into_owned(),
        ),
    ]);
    let selected =
        select_with_environment(&options, &fixture.context, &accounts, &environment).unwrap();
    assert_eq!(selected.source.account, "1");
    assert_eq!(selected.target.unwrap().account.account, "3");
    assert!(selected.check.advice.advised);
}

#[test]
fn selection_is_same_provider_for_every_supported_engine() {
    let fixture = fixture();
    for engine in Engine::ALL {
        let default_dir = match engine {
            Engine::Claude => ".claude",
            Engine::Codex => ".codex",
            Engine::Opencode => ".opencode",
            Engine::Kimi => ".kimi-code",
            Engine::Grok => ".grok",
        };
        let source_profile = fixture.context.paths.home.join(default_dir);
        let target_profile = fixture
            .context
            .paths
            .home
            .join(format!("{default_dir}-acct2"));
        let other_profile = fixture.context.paths.home.join(".other-acct2");
        let options = HandoffOptions {
            engine,
            source_account: None,
            target_selectors: Vec::new(),
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
        let accounts = vec![
            account(engine, "1", &source_profile.to_string_lossy(), 0.0, 99.0),
            account(engine, "2", &target_profile.to_string_lossy(), 0.0, 10.0),
            account(
                match engine {
                    Engine::Claude => Engine::Codex,
                    _ => Engine::Claude,
                },
                "2",
                &other_profile.to_string_lossy(),
                0.0,
                0.0,
            ),
        ];
        let environment = BTreeMap::from([
            ("NEOMAX_ROLE".into(), engine.to_string()),
            (
                neomax_core::orchestration::handoff::config_env(engine).into(),
                source_profile.to_string_lossy().into_owned(),
            ),
        ]);
        let selected =
            select_with_environment(&options, &fixture.context, &accounts, &environment).unwrap();
        assert_eq!(selected.target.unwrap().account.engine, engine);
    }
}
