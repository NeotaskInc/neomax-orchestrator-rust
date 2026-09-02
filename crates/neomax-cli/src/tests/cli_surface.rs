use neomax_core::Engine;
use neomax_core::orchestration::commands::{Command, Launcher, resolve};

use crate::cli;
use crate::launch::LaunchOptions;
use crate::tests::fixture;

#[test]
fn documented_command_aliases_resolve_to_their_canonical_handlers() {
    let aliases = [
        ("help", Command::Help),
        ("commands", Command::Help),
        ("select", Command::Select),
        ("why", Command::Why),
        ("dispatch", Command::Dispatch),
        ("auto", Command::Dispatch),
        ("list", Command::List),
        ("ls", Command::List),
        ("pr", Command::PullRequest),
        ("ack", Command::Acknowledge),
        ("orch-list", Command::Orchestrators),
        ("orchs", Command::Orchestrators),
        ("premerge-check", Command::Premerge),
        ("premerge", Command::Premerge),
        ("pick_orch", Command::PickOrchestrator),
        ("pick_neomax", Command::PickNeomax),
        ("orch_on", Command::OrchestratorOn),
        ("solo_rotate", Command::SoloRotate),
        ("solo_setup", Command::SoloSetup),
        ("session_rotate", Command::SessionRotate),
        ("rotate_tick", Command::RotateTick),
        ("subagent_diff", Command::SubagentDiff),
        ("register-project", Command::ProjectRegister),
        ("unregister-project", Command::ProjectUnregister),
        ("task", Command::Tasks),
        ("backlog", Command::Tasks),
        ("rotate_auth", Command::RotateAuth),
        ("usage_watch", Command::UsageWatch),
        ("keep-alive", Command::Keepalive),
        ("turn_hook", Command::TurnHook),
        ("model_guard", Command::ModelGuard),
        ("usage_hook", Command::UsageHook),
        ("runall", Command::RunAll),
    ];
    for (alias, expected) in aliases {
        assert_eq!(resolve(alias), Some(expected), "alias {alias}");
    }
}

#[test]
fn every_launcher_help_advertises_the_shared_option_aliases() {
    let launchers = [
        Launcher::Universal,
        Launcher::ProviderOrchestrator(Engine::Claude),
        Launcher::ProviderOrchestrator(Engine::Codex),
        Launcher::ProviderOrchestrator(Engine::Opencode),
        Launcher::ProviderOrchestrator(Engine::Kimi),
        Launcher::ProviderOrchestrator(Engine::Grok),
    ];
    for launcher in launchers {
        let help = cli::help_text(launcher);
        for option in [
            "--prefer",
            "--priority",
            "--orchestrator",
            "--dedicated",
            "--wait",
            "--foreground",
            "--fg",
            "--detach",
            "--run-id",
            "--tag",
            "--version",
            "-V",
            "--help",
            "-h",
        ] {
            assert!(help.contains(option), "{launcher:?} help lacks {option}");
        }
    }
}

#[test]
fn help_and_version_short_forms_match_long_forms() {
    for value in ["help", "commands", "--help", "-h"] {
        assert!(cli::is_help(&[value.to_owned()]), "help form {value}");
    }
    for value in ["--version", "-V"] {
        assert!(cli::is_version(&[value.to_owned()]), "version form {value}");
    }
    assert!(!cli::is_help(&["status".into()]));
    assert!(!cli::is_version(&["status".into()]));
}

#[test]
fn help_and_version_flags_after_the_delimiter_are_payload() {
    assert!(!cli::is_help(&["--".into(), "--help".into()]));
    assert!(!cli::is_help(&["task".into(), "--".into(), "-h".into()]));
    assert!(!cli::is_version(&["--".into(), "--version".into()]));
    assert!(!cli::is_version(&["task".into(), "--".into(), "-V".into()]));

    assert!(cli::is_help(&[
        "--help".into(),
        "--".into(),
        "payload".into()
    ]));
    assert!(cli::is_version(&[
        "--version".into(),
        "--".into(),
        "payload".into()
    ]));
}

#[test]
fn launch_option_aliases_produce_identical_routing_state() {
    let canonical = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--dry-run".into(),
            "--worker-dispatch".into(),
            "--engine".into(),
            "claude".into(),
            "--prefer".into(),
            "codex+opencode".into(),
            "--orchestrator".into(),
            "--foreground".into(),
            "worker task".into(),
        ],
    )
    .expect("canonical launch options");
    let aliases = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--dry-run".into(),
            "--worker-dispatch".into(),
            "--engine".into(),
            "claude".into(),
            "--priority".into(),
            "codex+opencode".into(),
            "--dedicated".into(),
            "--fg".into(),
            "worker task".into(),
        ],
    )
    .expect("alias launch options");

    assert_eq!(aliases.priority, canonical.priority);
    assert_eq!(aliases.dedicated, canonical.dedicated);
    assert_eq!(aliases.foreground, canonical.foreground);
    assert_eq!(aliases.detach, canonical.detach);
    assert_eq!(aliases.worker_dispatch, canonical.worker_dispatch);
    assert_eq!(aliases.positionals, canonical.positionals);
}

#[test]
fn every_provider_specific_model_flag_is_parseable_on_a_pinned_root() {
    let options = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Claude),
        &[
            "--dry-run".into(),
            "--workers".into(),
            "all".into(),
            "--model".into(),
            "claude-fable-5[1m]".into(),
            "--claude-model".into(),
            "claude-fable-5[1m]".into(),
            "--codex-model".into(),
            "gpt-5.6-sol".into(),
            "--opencode-model".into(),
            "opencode/big-pickle".into(),
            "--kimi-model".into(),
            "kimi-code/k3".into(),
            "--grok-model".into(),
            "grok-4.6".into(),
            "inspect model routing".into(),
        ],
    )
    .expect("provider-specific model flags on a pinned root");
    assert_eq!(options.provider_models.len(), 5);
    assert_eq!(options.positionals, ["inspect model routing"]);
}

#[test]
fn status_dispatch_accepts_pinned_launcher_options_before_the_command() {
    let fixture = fixture();
    for launcher in [
        Launcher::Universal,
        Launcher::ProviderOrchestrator(Engine::Claude),
        Launcher::ProviderOrchestrator(Engine::Codex),
        Launcher::ProviderOrchestrator(Engine::Opencode),
        Launcher::ProviderOrchestrator(Engine::Kimi),
        Launcher::ProviderOrchestrator(Engine::Grok),
    ] {
        let args = [
            "--workers".into(),
            "all".into(),
            "status".into(),
            "--json".into(),
        ];
        cli::execute(launcher, &args, &fixture.context)
            .unwrap_or_else(|error| panic!("{launcher:?} status dispatch failed: {error:#}"));
    }
}
