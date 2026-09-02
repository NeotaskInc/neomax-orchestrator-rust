use std::fs;

use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::tasks::TaskStore;

use crate::cli;
use crate::tests::fixture;

#[test]
fn help_is_available_without_state_discovery() {
    assert!(cli::is_help(&["help".into()]));
    assert!(cli::is_help(&["--help".into()]));
    assert!(!cli::is_help(&["task".into()]));
}

#[test]
fn every_multicall_surface_has_its_own_help_and_version_name() {
    let launchers = [
        (Launcher::Universal, "neomax"),
        (Launcher::ProviderOrchestrator(Engine::Claude), "cmax"),
        (Launcher::ProviderOrchestrator(Engine::Codex), "cdxmax"),
        (Launcher::ProviderOrchestrator(Engine::Opencode), "ocmax"),
        (Launcher::ProviderOrchestrator(Engine::Kimi), "kmax"),
        (Launcher::ProviderOrchestrator(Engine::Grok), "gmax"),
        (Launcher::AccountHelper(Engine::Codex), "cdx"),
        (Launcher::AccountHelper(Engine::Opencode), "ocx"),
        (Launcher::AccountHelper(Engine::Kimi), "kmx"),
        (Launcher::AccountHelper(Engine::Grok), "gmx"),
    ];
    for (launcher, name) in launchers {
        let help = crate::cli::help_text(launcher);
        if launcher == Launcher::Universal {
            assert!(help.starts_with("Neomax Orchestrator"));
        } else {
            assert!(help.starts_with(name), "help did not identify {name}");
        }
        assert!(help.contains(&format!("  {name}")));
        assert!(help.contains("--version"));
        assert!(help.contains("-V"));
        assert!(help.contains("--help"));
        assert!(help.contains("-h"));
        assert_eq!(crate::launch::invocation_name(launcher), name);
    }
    assert_eq!(crate::cli::version(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn account_helper_help_matches_provider_command_surfaces() {
    let codex = cli::help_text(Launcher::AccountHelper(Engine::Codex));
    assert!(codex.contains("cdx status\n"));
    assert!(!codex.contains("cdx status [ACCOUNT]"));
    assert!(!codex.contains("cdx models"));
    assert!(codex.contains("access-token"));
    assert!(codex.contains("device authentication"));
    assert!(codex.contains("reads the token from stdin"));

    let opencode = cli::help_text(Launcher::AccountHelper(Engine::Opencode));
    assert!(opencode.contains("ocx login ACCOUNT [PROVIDER] [oauth|api-key]"));
    assert!(opencode.contains("ocx models [ACCOUNT] [PROVIDER]"));
    assert!(opencode.contains("API-key and OAuth login"));

    let kimi = cli::help_text(Launcher::AccountHelper(Engine::Kimi));
    assert!(kimi.contains("kmx models [ACCOUNT]"));
    let grok = cli::help_text(Launcher::AccountHelper(Engine::Grok));
    assert!(grok.contains("gmx models [ACCOUNT]"));
}

#[test]
fn cmax_help_explains_its_pinned_profile_setup_without_advertising_a_generic_helper() {
    let help = cli::help_text(Launcher::ProviderOrchestrator(Engine::Claude));
    assert!(help.contains("cmax N /login"));
    assert!(help.contains("cmax orchestrator"));
    assert!(!help.contains("cmax models"));
}

#[test]
fn universal_help_lists_every_public_command_group() {
    let help = cli::help_text(Launcher::Universal);
    for command in [
        "help|commands",
        "config",
        "dispatch",
        "projects",
        "task|tasks|backlog",
        "queue",
        "select, why",
        "pause, unpause, paused",
        "orch-register",
        "orch-unregister",
        "pick-orch",
        "pick-neomax",
        "orch-on",
        "solo",
        "solo-setup",
        "rotate-auth",
        "usage-watch",
        "turn-hook",
        "model-guard",
        "usage-hook",
        "run-all",
        "ci-sync",
        "install, uninstall",
        "account status|pause|unpause|rotate",
        "Universal auxiliary executables:",
        "neomax-portal",
        "neomax-usage-agent",
        "neomax-worktrees",
    ] {
        assert!(help.contains(command), "universal help lacks {command}");
    }
}

#[test]
fn plan_help_states_the_read_only_boundary_and_dry_run_behavior() {
    let help = cli::help_text(Launcher::Universal);
    for guarantee in [
        "current checkout",
        "no managed worktree",
        "no write-mode provider permission",
        "no account or auth mutation",
        "A dry-run does not start any provider process",
        "live plan still requires worker authorization",
    ] {
        assert!(help.contains(guarantee), "plan help lacks {guarantee}");
    }
}

#[test]
fn concurrency_help_documents_both_persistent_setters() {
    for launcher in [
        Launcher::Universal,
        Launcher::ProviderOrchestrator(Engine::Claude),
    ] {
        let help = cli::help_text(launcher);
        assert!(help.contains("neomax config set max-subagents N"));
        assert!(help.contains("neomax config set max-sessions-per-account N"));
    }
}

#[test]
fn public_task_and_project_aliases_reach_the_same_handlers() {
    let fixture = fixture();
    cli::execute(
        Launcher::Universal,
        &["backlog".into(), "add".into(), "alias task".into()],
        &fixture.context,
    )
    .expect("backlog alias");
    assert!(
        TaskStore::new(&fixture.context.paths.tasks)
            .load()
            .tasks
            .contains_key("t1")
    );

    let root = fixture.context.cwd.join("alias-project");
    fs::create_dir_all(&root).expect("project root");
    cli::execute(
        Launcher::Universal,
        &[
            "register-project".into(),
            "--name".into(),
            "alias-project".into(),
            "--root".into(),
            root.to_str().expect("UTF-8 root").into(),
        ],
        &fixture.context,
    )
    .expect("register-project alias");
    assert!(
        fixture
            .context
            .project_registry()
            .load()
            .contains_key("aliasproject")
    );
}
