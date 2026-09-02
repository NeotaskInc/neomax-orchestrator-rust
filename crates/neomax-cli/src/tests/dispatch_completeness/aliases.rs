use neomax_core::Engine;
use neomax_core::agent_tools::ToolManifest;
use neomax_core::orchestration::commands::{Launcher, resolve};

const COMMAND_ALIASES: &[&str] = &[
    "help",
    "commands",
    "-h",
    "--help",
    "select",
    "why",
    "portal",
    "config",
    "delegate",
    "dispatch",
    "auto",
    "list",
    "ls",
    "log",
    "resume",
    "retry",
    "kill",
    "pr",
    "reconcile",
    "ack",
    "audit",
    "find",
    "history",
    "status",
    "pause",
    "unpause",
    "paused",
    "orchestrators",
    "orch-list",
    "orchs",
    "orch-register",
    "orch_register",
    "orch-unregister",
    "orch_unregister",
    "premerge-check",
    "premerge",
    "pick-orch",
    "pick_orch",
    "pick-neomax",
    "pick_neomax",
    "orch-on",
    "orch_on",
    "solo",
    "solo-rotate",
    "solo_rotate",
    "solo-setup",
    "solo_setup",
    "session-rotate",
    "session_rotate",
    "rotate",
    "rotate-tick",
    "rotate_tick",
    "handoff",
    "modes",
    "sessions",
    "subagents",
    "diff",
    "subagent-diff",
    "subagent_diff",
    "projects",
    "project-register",
    "project_register",
    "project-unregister",
    "project_unregister",
    "register-project",
    "unregister-project",
    "task",
    "tasks",
    "backlog",
    "rotate-auth",
    "orient",
    "usage",
    "usage-watch",
    "usage_watch",
    "keepalive",
    "keep-alive",
    "turn-hook",
    "turn_hook",
    "model-guard",
    "model_guard",
    "usage-hook",
    "usage_hook",
    "run-all",
    "runall",
    "shepherd",
    "issue",
    "ci-sync",
    "queue",
    "clean",
    "tidy",
    "install",
    "uninstall",
    "__supervise",
    "12",
];

pub(super) const MULTICALL_LAUNCHERS: &[(&str, Launcher)] = &[
    ("neomax", Launcher::Universal),
    ("neomax-cli", Launcher::Universal),
    ("cmax", Launcher::ProviderOrchestrator(Engine::Claude)),
    ("cdxmax", Launcher::ProviderOrchestrator(Engine::Codex)),
    ("ocmax", Launcher::ProviderOrchestrator(Engine::Opencode)),
    ("kmax", Launcher::ProviderOrchestrator(Engine::Kimi)),
    ("gmax", Launcher::ProviderOrchestrator(Engine::Grok)),
    ("cdx", Launcher::AccountHelper(Engine::Codex)),
    ("ocx", Launcher::AccountHelper(Engine::Opencode)),
    ("kmx", Launcher::AccountHelper(Engine::Kimi)),
    ("gmx", Launcher::AccountHelper(Engine::Grok)),
];

#[test]
fn every_reference_command_alias_is_registered() {
    let missing = COMMAND_ALIASES
        .iter()
        .copied()
        .filter(|alias| resolve(alias).is_none())
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "reference command aliases are not registered: {missing:?}"
    );
}

#[test]
fn canonical_manifest_additions_have_cli_resolution() {
    let manifest = ToolManifest::canonical();
    for name in ["portal", "select", "why", "install", "uninstall"] {
        assert!(manifest.command(name).is_some(), "manifest lacks {name}");
        assert!(
            resolve(name).is_some(),
            "CLI lacks resolver entry for {name}"
        );
    }
}

#[test]
fn every_multicall_alias_selects_the_expected_launcher() {
    for (alias, expected) in MULTICALL_LAUNCHERS {
        let actual = Launcher::from_argv0(std::ffi::OsStr::new(alias));
        assert_eq!(actual, Some(*expected), "launcher alias {alias}");
    }
}
