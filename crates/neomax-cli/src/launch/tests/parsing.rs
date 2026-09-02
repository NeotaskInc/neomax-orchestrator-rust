use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::runs::execution::MAX_TIMEOUT_MINUTES;

use super::super::LaunchOptions;

#[test]
fn conflicting_attachment_modes_fail_before_any_provider_selection() {
    let error = LaunchOptions::parse(
        Launcher::Universal,
        &["--dry-run".into(), "--detach".into(), "--foreground".into()],
    )
    .expect_err("attachment modes must be exclusive");
    assert!(error.to_string().contains("cannot be used together"));
}

#[test]
fn worker_metadata_accepts_fixed_ids_and_searchable_tags() {
    let options = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--worker-dispatch".into(),
            "--run-id=PLAN-p1".into(),
            "--tag".into(),
            "plan=PLAN".into(),
            "worker task".into(),
        ],
    )
    .unwrap();
    assert_eq!(options.run_id.as_deref(), Some("PLAN-p1"));
    assert_eq!(options.tag.as_deref(), Some("plan=PLAN"));
}

#[test]
fn fixed_run_id_keeps_worker_attached_by_default_but_detach_overrides() {
    let attached = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--worker-dispatch".into(),
            "--run-id=PLAN-p1".into(),
            "worker task".into(),
        ],
    )
    .unwrap();
    assert!(attached.foreground);
    assert!(!attached.detach);

    let detached = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--worker-dispatch".into(),
            "--run-id=PLAN-p1".into(),
            "--detach".into(),
            "worker task".into(),
        ],
    )
    .unwrap();
    assert!(!detached.foreground);
    assert!(detached.detach);
}

#[test]
fn worker_metadata_rejects_path_traversal_and_control_input() {
    for value in ["../run", ".", "run..part", "run/id", "run\\id"] {
        let error = LaunchOptions::parse(
            Launcher::Universal,
            &[
                "--worker-dispatch".into(),
                "--run-id".into(),
                value.into(),
                "worker task".into(),
            ],
        )
        .expect_err("unsafe run id must fail closed");
        assert!(error.to_string().contains("--run-id"));
    }

    for value in ["../tag", "tag\nnext", "tag\0value", "tag/value"] {
        let error = LaunchOptions::parse(
            Launcher::Universal,
            &[
                "--worker-dispatch".into(),
                "--tag".into(),
                value.into(),
                "worker task".into(),
            ],
        )
        .expect_err("unsafe tag must fail closed");
        assert!(error.to_string().contains("--tag"));
    }
}

#[test]
fn metadata_validation_returns_typed_invalid_argument_errors() {
    for args in [
        vec!["--run-id=".into(), "worker task".into()],
        vec!["--tag=".into(), "worker task".into()],
    ] {
        let error = LaunchOptions::parse(Launcher::Universal, &args)
            .expect_err("empty metadata must fail closed");
        assert!(matches!(
            error.downcast_ref::<neomax_core::Error>(),
            Some(neomax_core::Error::InvalidArgument(_))
        ));
    }
}

#[test]
fn timeout_parser_accepts_the_shared_ceiling_and_rejects_overflow_values() {
    let accepted = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--dry-run".into(),
            "-t".into(),
            MAX_TIMEOUT_MINUTES.to_string(),
            "-s".into(),
            "0".into(),
        ],
    )
    .unwrap();
    assert_eq!(accepted.wall_min, Some(MAX_TIMEOUT_MINUTES));
    assert_eq!(accepted.stall_min, Some(0.0));

    for value in [MAX_TIMEOUT_MINUTES + 1.0, f64::MAX, f64::INFINITY] {
        let error = LaunchOptions::parse(
            Launcher::Universal,
            &["--dry-run".into(), "-t".into(), value.to_string()],
        )
        .expect_err("unrepresentable timeout should be rejected");
        assert!(error.to_string().contains("between 0 and"));
    }
}

#[test]
fn canonical_dispatch_marker_forces_worker_mode_on_pinned_launchers() {
    let options = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Opencode),
        &[
            "--dry-run".into(),
            "--worker-dispatch".into(),
            "--foreground".into(),
            "worker task".into(),
        ],
    )
    .unwrap();
    assert!(options.worker_dispatch);
    assert_eq!(options.positionals, ["worker task"]);
}

#[test]
fn delimiter_preserves_help_and_version_tokens_as_payload_on_every_launcher() {
    let launchers = [
        Launcher::Universal,
        Launcher::ProviderOrchestrator(Engine::Claude),
        Launcher::ProviderOrchestrator(Engine::Codex),
        Launcher::ProviderOrchestrator(Engine::Opencode),
        Launcher::ProviderOrchestrator(Engine::Kimi),
        Launcher::ProviderOrchestrator(Engine::Grok),
        Launcher::AccountHelper(Engine::Codex),
        Launcher::AccountHelper(Engine::Opencode),
        Launcher::AccountHelper(Engine::Kimi),
        Launcher::AccountHelper(Engine::Grok),
    ];
    for launcher in launchers {
        let options = LaunchOptions::parse(
            launcher,
            &[
                "--foreground".into(),
                "--".into(),
                "--help".into(),
                "-V".into(),
            ],
        )
        .unwrap_or_else(|error| panic!("{launcher:?} delimiter parse failed: {error:#}"));
        assert!(!options.routing_allowed, "{launcher:?} re-enabled routing");
        assert_eq!(options.positionals, ["--help", "-V"], "{launcher:?}");
    }
}

#[test]
fn direct_worker_model_flags_require_the_selected_engine() {
    let foreign = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--worker-dispatch".into(),
            "--engine".into(),
            "codex".into(),
            "--claude-model".into(),
            "fixture/claude".into(),
            "worker task".into(),
        ],
    )
    .expect_err("foreign worker model should be rejected");
    assert!(foreign.to_string().contains("--claude-model"));
    assert!(foreign.to_string().contains("--engine claude"));

    let dynamic = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--worker-dispatch".into(),
            "--opencode-model".into(),
            "fixture/opencode".into(),
            "worker task".into(),
        ],
    )
    .expect_err("dynamic direct worker model should require an engine");
    assert!(dynamic.to_string().contains("--engine opencode"));

    LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--worker-dispatch".into(),
            "--workers=opencode".into(),
            "--opencode-model=fixture/opencode".into(),
            "worker task".into(),
        ],
    )
    .expect("a single-provider worker scope identifies the target engine");
}

#[test]
fn direct_worker_generic_and_specific_models_must_agree() {
    let error = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--worker-dispatch".into(),
            "--engine=codex".into(),
            "--model=fixture/generic".into(),
            "--codex-model=fixture/specific".into(),
            "worker task".into(),
        ],
    )
    .expect_err("conflicting worker model flags should be rejected");
    assert!(
        error
            .to_string()
            .contains("--model and --codex-model disagree")
    );

    LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--worker-dispatch".into(),
            "--engine=codex".into(),
            "--model=fixture/same".into(),
            "--codex-model=fixture/same".into(),
            "worker task".into(),
        ],
    )
    .expect("equal generic and provider-specific models are unambiguous");
}

#[test]
fn pinned_root_allows_foreign_worker_pool_model_overrides() {
    let options = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Claude),
        &[
            "--workers=codex+opencode".into(),
            "--codex-model=fixture/codex".into(),
            "--opencode-model=fixture/opencode".into(),
            "coordinate worker pool".into(),
        ],
    )
    .expect("pinned root must preserve worker-pool model overrides");
    assert!(!options.worker_dispatch);
    assert_eq!(
        options
            .provider_models
            .get(&Engine::Codex)
            .map(String::as_str),
        Some("fixture/codex")
    );
    assert_eq!(
        options
            .provider_models
            .get(&Engine::Opencode)
            .map(String::as_str),
        Some("fixture/opencode")
    );
}

#[test]
fn cmax_account_and_orchestrator_setup_keep_claude_profile_semantics() {
    let account = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Claude),
        &["2".into()],
    )
    .unwrap();
    assert_eq!(account.account.as_deref(), Some("2"));
    assert_eq!(account.routing, "account");
    assert!(!account.worker_dispatch);

    let login = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Claude),
        &["2".into(), "/login".into()],
    )
    .unwrap();
    assert_eq!(login.account.as_deref(), Some("2"));
    assert_eq!(login.positionals, ["/login"]);
    assert!(!login.worker_dispatch);

    let orchestrator = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Claude),
        &["orchestrator".into()],
    )
    .unwrap();
    assert_eq!(orchestrator.account.as_deref(), Some("orch"));
    assert!(orchestrator.positionals.is_empty());
    assert!(!orchestrator.dedicated);

    let short_orchestrator = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Claude),
        &["orch".into()],
    )
    .unwrap();
    assert_eq!(short_orchestrator.account.as_deref(), Some("orch"));
    assert!(short_orchestrator.positionals.is_empty());
}

#[test]
fn codex_short_model_flag_is_the_provider_model_alias() {
    let options = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Codex),
        &["-cm".into(), "fixture/codex-model".into(), "task".into()],
    )
    .unwrap();
    assert_eq!(
        options
            .provider_models
            .get(&Engine::Codex)
            .map(String::as_str),
        Some("fixture/codex-model")
    );
}

#[test]
fn inline_resume_flag_carries_the_native_session_selector() {
    let options = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Grok),
        &["--resume=session-42".into()],
    )
    .unwrap();
    assert!(options.resume);
    assert_eq!(options.session_id.as_deref(), Some("session-42"));
}
