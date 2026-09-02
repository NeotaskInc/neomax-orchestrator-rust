#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::{
    E2eHarness,
    assertions::{assert_not_worker_tagged, assert_orchestrator_tools, assert_root_argv},
};

#[test]
fn universal_and_pinned_orchestrators_handle_initial_tasks_by_capability() {
    let mut cases = Engine::ALL
        .into_iter()
        .map(|engine| ("neomax", engine))
        .collect::<Vec<_>>();
    cases.extend([
        ("cmax", Engine::Claude),
        ("cdxmax", Engine::Codex),
        ("ocmax", Engine::Opencode),
        ("kmax", Engine::Kimi),
        ("gmax", Engine::Grok),
    ]);

    for (launcher, engine) in cases {
        let harness = E2eHarness::new([engine]);
        let args: Vec<String> = if engine == Engine::Kimi {
            vec!["--json".into(), "--foreground".into()]
        } else {
            vec![
                "--json".into(),
                "--foreground".into(),
                "fixture task".into(),
            ]
        };
        let result = harness.run_alias(launcher, args);
        let report = result.json();
        assert_eq!(report["status"], "done", "launcher {launcher}: {report}");
        assert_eq!(report["engine"], engine.as_str(), "launcher {launcher}");
        let invocation = harness
            .invocations()
            .pop()
            .unwrap_or_else(|| panic!("{launcher} did not invoke its provider"));
        assert_orchestrator_tools(&invocation, launcher);
        assert_root_argv(&invocation, engine, launcher);
        assert_not_worker_tagged(&invocation, launcher);
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn kimi_initial_tasks_bootstrap_then_resume_the_interactive_provider() {
    for launcher in ["neomax", "kmax"] {
        let harness = E2eHarness::new([Engine::Kimi]);
        let result = harness.run_alias(launcher, ["--json", "--foreground", "fixture task"]);
        let report = result.json();
        assert_eq!(report["status"], "done", "launcher {launcher}: {report}");
        assert_eq!(report["engine"], "kimi", "launcher {launcher}: {report}");
        let invocations = harness.invocations();
        assert_eq!(
            invocations.len(),
            2,
            "{launcher} should bootstrap then resume"
        );
        let bootstrap = &invocations[0];
        assert_eq!(bootstrap.field("provider"), Some("kimi"));
        assert!(bootstrap.has_arg("--prompt"));
        assert!(!bootstrap.has_arg("--auto"));
        assert_eq!(
            bootstrap.arg_value("--prompt"),
            Some(
                "You are the Neomax orchestrator for this project. Preserve and follow the project's instructions. Use NEOMAX_BIN with the canonical commands in NEOMAX_TOOL_MANIFEST; inspect status, runs, usage, and account eligibility before routing work. Dispatch and coordinate workers across the configured scope, verify their results, and keep the task moving. Honor NEOMAX_TOOL_POLICY and NEOMAX_TOOL_DEPTH/NEOMAX_TOOL_MAX_DEPTH; never bypass the manifest or recursion policy. Do not behave as a delegated worker and do not ask the user to perform orchestration that the available tools can perform.\n\nfixture task"
            )
        );
        let interactive = &invocations[1];
        assert_eq!(interactive.field("provider"), Some("kimi"));
        assert!(!interactive.has_arg("--prompt"));
        assert!(interactive.has_arg("--auto"));
        assert_eq!(interactive.arg_value("-S"), Some("session-kimi"));
        assert_root_argv(interactive, Engine::Kimi, launcher);
        assert_orchestrator_tools(interactive, launcher);
        assert_not_worker_tagged(interactive, launcher);
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn universal_launch_selects_the_only_connected_provider_and_runs_the_fake_cli() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run(["--json", "--foreground", "ship", "the", "fixture"]);
    let report = result.json();

    assert_eq!(report["status"], "done", "report: {report}");
    assert_eq!(report["engine"], "opencode");
    let account = harness
        .profile(Engine::Opencode, 0)
        .file_name()
        .and_then(|name| name.to_str())
        .expect("fixture account name");
    assert_eq!(report["account"], account);
    assert_eq!(report["model"], "opencode/big-pickle");
    assert_eq!(report["worker_scope"], "claude,codex,opencode,kimi,grok");

    let invocations = harness.invocations();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].field("provider"), Some("opencode"));
    assert_root_argv(&invocations[0], Engine::Opencode, "neomax");
    assert_not_worker_tagged(&invocations[0], "neomax");
    assert!(invocations[0].has_arg("--model"));
    assert_eq!(
        invocations[0].field("network_proxy"),
        Some("http://127.0.0.1:9")
    );
    harness.assert_hermetic_invocations();
}

#[test]
fn each_provider_pinned_launcher_selects_its_provider_without_authenticating_any_other() {
    let cases = [
        ("cmax", Engine::Claude, "claude-fable-5[1m]"),
        ("cdxmax", Engine::Codex, "gpt-5.6-sol"),
        ("ocmax", Engine::Opencode, "opencode/big-pickle"),
        ("kmax", Engine::Kimi, "kimi-code/k3"),
        ("gmax", Engine::Grok, "grok-4.6"),
    ];

    for (launcher, engine, model) in cases {
        let harness = E2eHarness::new([engine]);
        let args: Vec<String> = if engine == Engine::Kimi {
            vec!["--json".into(), "--foreground".into()]
        } else {
            vec![
                "--json".into(),
                "--foreground".into(),
                "fixture task".into(),
            ]
        };
        let result = harness.run_alias(launcher, args);
        let report = result.json();
        assert_eq!(report["engine"], engine.as_str(), "launcher {launcher}");
        assert_eq!(report["model"], model, "launcher {launcher}");
        assert_eq!(report["status"], "done", "launcher {launcher}");
        let invocations = harness.invocations();
        assert_eq!(invocations.len(), 1, "launcher {launcher}");
        assert_eq!(invocations[0].field("provider"), Some(engine.as_str()));
        assert_root_argv(&invocations[0], engine, launcher);
        assert_not_worker_tagged(&invocations[0], launcher);
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn pinned_orchestrator_keeps_worker_pool_scope_independent() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run_alias(
        "ocmax",
        [
            "--json",
            "--foreground",
            "--workers",
            "codex+grok",
            "--opencode-model",
            "local/fixture/model",
            "coordinate fixture",
        ],
    );
    let report = result.json();
    assert_eq!(report["engine"], "opencode");
    assert_eq!(report["worker_scope"], "codex,grok");
    assert_eq!(report["model"], "local/fixture/model");
    assert_eq!(harness.invocations().len(), 1);
    let invocation = harness.invocations().pop().expect("ocmax invocation");
    assert_root_argv(&invocation, Engine::Opencode, "ocmax");
    assert_not_worker_tagged(&invocation, "ocmax");
    harness.assert_hermetic_invocations();
}

#[test]
fn cmax_solo_uses_a_plain_claude_session_and_arms_local_rotation() {
    let harness = E2eHarness::new([Engine::Claude]);
    let result = harness.run_alias(
        "cmax",
        ["solo", "--json", "--foreground", "solo fixture task"],
    );
    let report = result.json();
    assert_eq!(report["status"], "done");
    assert_eq!(report["engine"], "claude");
    assert_eq!(report["model"], "claude-fable-5[1m]");

    let invocations = harness.invocations();
    assert_eq!(invocations.len(), 1);
    let invocation = &invocations[0];
    assert_eq!(invocation.field("mode"), Some("solo"));
    assert_eq!(invocation.field("role"), Some(""));
    assert_eq!(invocation.field("worker"), Some(""));
    assert!(invocation.field("profile").is_some_and(|profile| {
        profile.ends_with("/.claude-solo") || profile.ends_with("\\.claude-solo")
    }));
    assert!(invocation.has_arg("--dangerously-skip-permissions"));
    assert!(invocation.has_arg("--settings"));
    assert!(invocation.has_arg("--effort"));
    assert!(!invocation.has_arg("--append-system-prompt"));
    assert!(
        !invocation
            .args
            .iter()
            .any(|argument| argument.contains("You are the Neomax orchestrator"))
    );

    let armed = std::fs::read_to_string(harness.state.join("armed-rotate.json"))
        .expect("solo launch arms the rotation state");
    assert!(armed.contains(".claude-solo"));
    harness.assert_hermetic_invocations();
}

#[test]
fn provider_pinned_solo_aliases_start_plain_sessions_without_neomax_tools() {
    let cases = [
        ("neomax", Engine::Opencode),
        ("cdxmax", Engine::Codex),
        ("ocmax", Engine::Opencode),
        ("kmax", Engine::Kimi),
        ("gmax", Engine::Grok),
    ];

    for (alias, engine) in cases {
        let harness = E2eHarness::new([engine]);
        let args: Vec<String> = if engine == Engine::Kimi {
            vec!["solo".into(), "--json".into(), "--foreground".into()]
        } else {
            vec![
                "solo".into(),
                "--json".into(),
                "--foreground".into(),
                "solo fixture task".into(),
            ]
        };
        let result = harness.run_alias(alias, args);
        let report = result.json();
        assert_eq!(report["status"], "done", "launcher {alias}: {report}");
        assert_eq!(report["engine"], engine.as_str(), "launcher {alias}");

        let invocations = harness.invocations();
        assert_eq!(invocations.len(), 1, "launcher {alias}");
        let invocation = &invocations[0];
        assert_eq!(invocation.field("mode"), Some("solo"), "launcher {alias}");
        assert_eq!(invocation.field("role"), Some(""), "launcher {alias}");
        assert_eq!(invocation.field("worker"), Some(""), "launcher {alias}");
        assert_eq!(
            invocation.field("tool_policy"),
            Some(""),
            "launcher {alias}"
        );
        assert_eq!(
            invocation.field("tool_manifest"),
            Some(""),
            "launcher {alias}"
        );
        assert_root_argv(invocation, engine, alias);
        harness.assert_hermetic_invocations();
    }
}
