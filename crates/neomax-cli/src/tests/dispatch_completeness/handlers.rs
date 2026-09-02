use neomax_core::agent_tools::CANONICAL_COMMANDS;
use neomax_core::orchestration::commands::resolve;

use crate::cli;
use crate::tests::fixture;

#[test]
fn every_reference_command_reaches_a_concrete_handler() {
    let fixture = fixture();
    let cwd = fixture.context.cwd.to_string_lossy().into_owned();
    let cases = dispatch_cases(&cwd);
    let mut gaps = Vec::new();

    for (alias, args) in cases {
        let command = resolve(alias).unwrap_or_else(|| panic!("unregistered alias {alias}"));
        let result = cli::execute(
            neomax_core::orchestration::commands::Launcher::Universal,
            &args,
            &fixture.context,
        );
        if let Err(error) = result {
            if is_dispatch_gap(&error.to_string()) {
                gaps.push(format!("{alias} ({command:?}): {error:#}"));
            }
        }
    }

    assert!(
        gaps.is_empty(),
        "reference command dispatch gaps detected:\n{}",
        gaps.join("\n")
    );
}

#[test]
fn every_canonical_manifest_command_reaches_a_handler_or_safe_dispatch() {
    let fixture = fixture();
    let cwd = fixture.context.cwd.to_string_lossy().into_owned();
    let mut gaps = Vec::new();

    for canonical in CANONICAL_COMMANDS {
        let args = canonical_args(canonical.command, &cwd);
        let result = cli::execute(
            neomax_core::orchestration::commands::Launcher::Universal,
            &args,
            &fixture.context,
        );
        if let Err(error) = result {
            if is_dispatch_gap(&error.to_string()) {
                gaps.push(format!("{}: {error:#}", canonical.command));
            }
        }
    }

    assert!(
        gaps.is_empty(),
        "canonical manifest dispatch gaps detected:\n{}",
        gaps.join("\n")
    );
}

fn canonical_args(command: &str, cwd: &str) -> Vec<String> {
    let values = match command {
        "help" => vec!["help"],
        "config show" => vec!["config", "show", "--json"],
        "config set" => vec!["config", "set", "max-subagents", "1", "--json"],
        "dispatch" => vec![
            "dispatch",
            "--dry-run",
            "--foreground",
            "--engine",
            "opencode",
            "manifest dispatch",
        ],
        "account status" => vec!["account", "status", "--json"],
        "account pause" => vec!["account", "pause", "all", "--engine", "claude", "--json"],
        "account unpause" => vec!["account", "unpause", "all", "--engine", "claude", "--json"],
        "account rotate" => vec!["account", "rotate", "--dry-run", "--json"],
        "pause" => vec!["pause", "all", "--json"],
        "unpause" => vec!["unpause", "all", "--json"],
        "diff" => vec!["diff", "missing-run", "--json"],
        "subagent-diff" => vec!["subagent-diff", "missing-run", "--json"],
        "premerge" => vec!["premerge", "--repo", cwd, "--json"],
        "clean" => vec!["clean", "--done", "--json"],
        "tidy" => vec!["tidy", "--json"],
        "ls" => vec!["ls", "--json"],
        "issue" => vec!["issue", "list", "--json"],
        "ci-sync" => vec!["ci-sync", "--json"],
        "ack" => vec!["ack", "--all", "--json"],
        "reconcile" => vec!["reconcile", "--json"],
        "resume" => vec!["resume", "missing-run", "--json"],
        "retry" => vec!["retry", "missing-run", "--json"],
        "kill" => vec!["kill", "missing-run", "--json"],
        "handoff" => vec![
            "handoff",
            "--engine",
            "claude",
            "--from",
            "1",
            "--target-account",
            "2",
            "--base",
            cwd,
            "--dry-run",
            "--json",
        ],
        "shepherd" => vec!["shepherd", "--repo", cwd, "--json"],
        "orchestrators" => vec!["orchestrators", "--json"],
        "modes" => vec!["modes", "--json"],
        "pick-orch" => vec!["pick-orch", "--dry-run", "--json"],
        "pick-neomax" => vec!["pick-neomax", "--dry-run", "--json"],
        "orch-register" => vec!["orch-register"],
        "orch-unregister" => vec!["orch-unregister"],
        "orch-on" => vec!["orch-on"],
        "orient" => vec!["orient", "--json"],
        "select" => vec!["select", "--json"],
        "why" => vec!["why", "--json"],
        "run-all" => vec!["run-all", "missing-plan.json"],
        "rotate" => vec!["rotate", "--dry-run", "--json"],
        "rotate-tick" => vec!["rotate-tick", "--dry-run", "--json"],
        "solo-rotate" => vec!["solo-rotate", "--json"],
        "solo-setup" => vec!["solo-setup"],
        "projects" => vec!["projects", "--json"],
        "project-register" => vec!["project-register"],
        "project-unregister" => vec!["project-unregister"],
        "queue" => vec!["queue", "status", "--json"],
        "sessions" => vec!["sessions", "--json"],
        "session-rotate" => vec!["session-rotate", "--json"],
        "subagents" => vec!["subagents", "--json"],
        "portal" => vec!["portal", "--unsupported-contract-option"],
        "tasks" => vec!["tasks", "list", "--json"],
        "task" => vec!["task", "list", "--json"],
        "usage" => vec!["usage", "--json"],
        // Keep this dispatch-only case from resolving a real service binary.
        "usage-watch" => vec!["usage-watch", "--unsupported-contract-option"],
        "keepalive" => vec!["keepalive", "--once"],
        "log" => vec!["log", "missing-run", "--json"],
        "audit" => vec!["audit", "--json"],
        "find" => vec!["find", "fixture", "--json"],
        "history" => vec!["history", "--json"],
        "status" => vec!["status", "--json"],
        "paused" => vec!["paused", "--json"],
        "rotate-auth" => vec!["rotate-auth", "--dry-run", "--json"],
        "pr" => vec!["pr", "--repo", cwd, "--branch", "main", "--json"],
        "turn-hook" => vec!["turn-hook"],
        "model-guard" => vec!["model-guard"],
        "usage-hook" => vec!["usage-hook"],
        "install" => vec!["install", "--unsupported"],
        "uninstall" => vec!["uninstall", "--unsupported"],
        other => panic!("missing hermetic canonical command fixture for {other}"),
    };
    values.into_iter().map(str::to_owned).collect()
}

fn dispatch_cases(cwd: &str) -> Vec<(&'static str, Vec<String>)> {
    vec![
        ("help", vec!["help".into()]),
        (
            "portal",
            vec!["portal".into(), "--unsupported-contract-option".into()],
        ),
        (
            "config",
            vec!["config".into(), "show".into(), "--json".into()],
        ),
        (
            "delegate",
            vec![
                "delegate".into(),
                "--dry-run".into(),
                "--json".into(),
                "dispatch contract".into(),
            ],
        ),
        (
            "dispatch",
            vec!["dispatch".into(), "--dry-run".into(), "--json".into()],
        ),
        ("list", vec!["list".into(), "--json".into()]),
        ("ls", vec!["ls".into(), "--json".into()]),
        (
            "log",
            vec!["log".into(), "missing-run".into(), "--json".into()],
        ),
        (
            "resume",
            vec!["resume".into(), "missing-run".into(), "--json".into()],
        ),
        (
            "retry",
            vec!["retry".into(), "missing-run".into(), "--json".into()],
        ),
        (
            "kill",
            vec!["kill".into(), "missing-run".into(), "--json".into()],
        ),
        (
            "pr",
            vec![
                "pr".into(),
                "--repo".into(),
                cwd.into(),
                "--branch".into(),
                "main".into(),
                "--json".into(),
            ],
        ),
        ("reconcile", vec!["reconcile".into(), "--json".into()]),
        ("ack", vec!["ack".into(), "--all".into(), "--json".into()]),
        ("audit", vec!["audit".into(), "--json".into()]),
        (
            "find",
            vec!["find".into(), "fixture".into(), "--json".into()],
        ),
        ("history", vec!["history".into(), "--json".into()]),
        ("status", vec!["status".into(), "--json".into()]),
        ("pause", vec!["pause".into(), "all".into(), "--json".into()]),
        (
            "unpause",
            vec!["unpause".into(), "all".into(), "--json".into()],
        ),
        ("paused", vec!["paused".into(), "--json".into()]),
        (
            "orchestrators",
            vec!["orchestrators".into(), "--json".into()],
        ),
        ("orch-register", vec!["orch-register".into()]),
        ("orch-unregister", vec!["orch-unregister".into()]),
        (
            "premerge-check",
            vec![
                "premerge-check".into(),
                "--repo".into(),
                cwd.into(),
                "--json".into(),
            ],
        ),
        ("pick-orch", vec!["pick-orch".into()]),
        ("pick-neomax", vec!["pick-neomax".into()]),
        ("orch-on", vec!["orch-on".into()]),
        (
            "solo",
            vec![
                "solo".into(),
                "--dry-run".into(),
                "--json".into(),
                "--engine".into(),
                "opencode".into(),
            ],
        ),
        ("solo-rotate", vec!["solo-rotate".into(), "--json".into()]),
        ("solo-setup", vec!["solo-setup".into()]),
        (
            "session-rotate",
            vec!["session-rotate".into(), "--json".into()],
        ),
        ("rotate", vec!["rotate".into(), "--json".into()]),
        ("rotate-tick", vec!["rotate-tick".into(), "--json".into()]),
        (
            "handoff",
            vec![
                "handoff".into(),
                "--engine".into(),
                "claude".into(),
                "--from".into(),
                "1".into(),
                "--target-account".into(),
                "2".into(),
                "--base".into(),
                cwd.into(),
                "--json".into(),
            ],
        ),
        ("modes", vec!["modes".into()]),
        ("sessions", vec!["sessions".into(), "--json".into()]),
        ("subagents", vec!["subagents".into(), "--json".into()]),
        (
            "diff",
            vec!["diff".into(), "missing-run".into(), "--json".into()],
        ),
        (
            "subagent-diff",
            vec![
                "subagent-diff".into(),
                "missing-run".into(),
                "--json".into(),
            ],
        ),
        ("projects", vec!["projects".into(), "--json".into()]),
        ("project-register", vec!["project-register".into()]),
        ("project-unregister", vec!["project-unregister".into()]),
        ("task", vec!["task".into(), "list".into(), "--json".into()]),
        ("rotate-auth", vec!["rotate-auth".into(), "--json".into()]),
        ("orient", vec!["orient".into()]),
        ("usage", vec!["usage".into(), "--json".into()]),
        (
            "usage-watch",
            vec!["usage-watch".into(), "--unsupported-contract-option".into()],
        ),
        ("keepalive", vec!["keepalive".into(), "--once".into()]),
        ("turn-hook", vec!["turn-hook".into()]),
        ("model-guard", vec!["model-guard".into()]),
        ("usage-hook", vec!["usage-hook".into()]),
        (
            "run-all",
            vec!["run-all".into(), "missing-plan.json".into()],
        ),
        (
            "shepherd",
            vec![
                "shepherd".into(),
                "--repo".into(),
                cwd.into(),
                "--json".into(),
            ],
        ),
        (
            "issue",
            vec!["issue".into(), "list".into(), "--json".into()],
        ),
        ("ci-sync", vec!["ci-sync".into(), "--json".into()]),
        (
            "queue",
            vec!["queue".into(), "status".into(), "--json".into()],
        ),
        (
            "clean",
            vec!["clean".into(), "--done".into(), "--json".into()],
        ),
        ("tidy", vec!["tidy".into(), "--json".into()]),
        ("install", vec!["install".into(), "--unsupported".into()]),
        (
            "uninstall",
            vec!["uninstall".into(), "--unsupported".into()],
        ),
        (
            "__supervise",
            vec!["__supervise".into(), "missing-run".into(), "--json".into()],
        ),
    ]
}

pub(super) fn is_dispatch_gap(error: &str) -> bool {
    error.contains("not wired to a local adapter")
        || error.contains("not owned by")
        || error.contains("unsupported run lifecycle command")
}
