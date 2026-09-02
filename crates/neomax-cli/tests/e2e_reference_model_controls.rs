#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::E2eHarness;

#[test]
fn codex_ultra_selects_xhigh_reasoning_without_the_claude_ultra_marker() {
    let harness = E2eHarness::new([Engine::Codex]);
    let result = harness.run_with_env(
        [
            "dispatch",
            "--json",
            "--foreground",
            "--brief",
            "--engine",
            "codex",
            "-u",
            "--goal",
            "the fixture is verified",
            "worker fixture",
        ],
        harness.authorized_orchestrator_environment(),
    );
    let report = result.json();
    assert_eq!(report["engine"], "codex");
    let invocation = harness.invocations().pop().expect("Codex invocation");
    assert!(invocation.has_arg("model_reasoning_effort=xhigh"));
    assert!(!invocation.has_arg("ultracode=true"));
    harness.assert_hermetic_invocations();
}

#[test]
fn root_claude_and_codex_effort_settings_reach_native_argv() {
    let claude = E2eHarness::new([Engine::Claude]);
    let result = claude.run_alias(
        "cmax",
        ["--json", "--foreground", "-e", "max", "-u", "fixture task"],
    );
    result.assert_success();
    let invocation = claude.invocations().pop().expect("Claude invocation");
    assert_eq!(invocation.arg_value("--effort"), Some("max"));
    assert_eq!(
        invocation.arg_value("--settings"),
        Some(r#"{"ultracode":true}"#)
    );
    claude.assert_hermetic_invocations();

    let codex = E2eHarness::new([Engine::Codex]);
    let result = codex.run_alias(
        "cdxmax",
        ["--json", "--foreground", "-e", "medium", "fixture task"],
    );
    result.assert_success();
    let invocation = codex.invocations().pop().expect("Codex invocation");
    assert!(invocation.has_arg("model_reasoning_effort=medium"));
    codex.assert_hermetic_invocations();

    let codex_ultra = E2eHarness::new([Engine::Codex]);
    let result = codex_ultra.run_alias("cdxmax", ["--json", "--foreground", "-u", "fixture task"]);
    result.assert_success();
    let invocation = codex_ultra
        .invocations()
        .pop()
        .expect("Codex ultra invocation");
    assert!(invocation.has_arg("model_reasoning_effort=xhigh"));
    codex_ultra.assert_hermetic_invocations();
}

#[test]
fn unsupported_provider_effort_and_ultra_flags_fail_before_provider_execution() {
    for (alias, flag) in [
        ("ocmax", "-e"),
        ("ocmax", "-u"),
        ("kmax", "-e"),
        ("kmax", "-u"),
        ("gmax", "-e"),
        ("gmax", "-u"),
        ("gmax", "--opus"),
    ] {
        let harness = E2eHarness::new([match alias {
            "ocmax" => Engine::Opencode,
            "kmax" => Engine::Kimi,
            "gmax" => Engine::Grok,
            _ => unreachable!(),
        }]);
        let args = if flag == "-e" {
            vec!["--json", "--foreground", flag, "high"]
        } else {
            vec!["--json", "--foreground", flag]
        };
        let result = harness.run_alias(alias, args);
        assert!(!result.status.success(), "{alias} accepted {flag}");
        let error = format!("{}\n{}", result.stdout, result.stderr);
        assert!(error.contains("do not apply") || error.contains("only valid"));
        assert!(
            harness.invocations().is_empty(),
            "{alias} started a provider"
        );
    }

    let harness = E2eHarness::new([Engine::Codex]);
    let result = harness.run_alias("cdxmax", ["--json", "--foreground", "-e", "max"]);
    assert!(!result.status.success());
    assert!(format!("{}\n{}", result.stdout, result.stderr).contains("Codex effort"));
    assert!(harness.invocations().is_empty());
}
