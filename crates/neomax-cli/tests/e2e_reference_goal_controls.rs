#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::E2eHarness;

#[test]
fn root_goal_and_max_turns_reach_provider_native_startup_commands() {
    let claude = E2eHarness::new([Engine::Claude]);
    let result = claude.run_alias(
        "cmax",
        [
            "--json",
            "--foreground",
            "--goal",
            "tests pass",
            "--max-turns",
            "3",
        ],
    );
    result.assert_success();
    let invocation = claude.invocations().pop().expect("Claude invocation");
    assert_eq!(invocation.arg_value("--max-turns"), Some("3"));
    assert!(invocation.args.iter().any(|arg| arg == "/goal tests pass"));
    claude.assert_hermetic_invocations();

    let codex = E2eHarness::new([Engine::Codex]);
    let result = codex.run_alias(
        "cdxmax",
        [
            "--json",
            "--foreground",
            "--goal",
            "tests pass",
            "--max-turns",
            "3",
        ],
    );
    result.assert_success();
    let invocation = codex.invocations().pop().expect("Codex invocation");
    let prompt = invocation.args.last().expect("Codex root prompt");
    assert!(prompt.contains("OBJECTIVE: do not finish until this condition holds:"));
    assert!(prompt.contains("Make at most 3 rounds of self-correction"));
    codex.assert_hermetic_invocations();

    let opencode = E2eHarness::new([Engine::Opencode]);
    let result = opencode.run_alias(
        "ocmax",
        [
            "--json",
            "--foreground",
            "--goal",
            "tests pass",
            "--max-turns",
            "3",
        ],
    );
    result.assert_success();
    let invocation = opencode.invocations().pop().expect("OpenCode invocation");
    let prompt = invocation.args.last().expect("OpenCode root prompt");
    assert!(prompt.contains("OBJECTIVE: do not finish until this condition holds:"));
    assert!(prompt.contains("Make at most 3 rounds of self-correction"));
    opencode.assert_hermetic_invocations();

    let grok = E2eHarness::new([Engine::Grok]);
    let result = grok.run_alias(
        "gmax",
        [
            "--json",
            "--foreground",
            "--goal",
            "tests pass",
            "--max-turns",
            "3",
        ],
    );
    result.assert_success();
    let invocation = grok.invocations().pop().expect("Grok invocation");
    assert_eq!(invocation.arg_value("--max-turns"), Some("3"));
    assert!(
        invocation
            .args
            .last()
            .is_some_and(|prompt| prompt.contains("OBJECTIVE: do not finish"))
    );
    grok.assert_hermetic_invocations();
}

#[test]
fn max_turns_requires_an_explicit_goal() {
    let harness = E2eHarness::new([Engine::Claude]);
    let result = harness.run_alias("cmax", ["--json", "--foreground", "--max-turns", "3"]);
    assert!(!result.status.success());
    assert!(format!("{}\n{}", result.stdout, result.stderr).contains("--goal"));
    assert!(harness.invocations().is_empty());
}

#[test]
fn kimi_root_goal_is_rejected_before_provider_execution() {
    let harness = E2eHarness::new([Engine::Kimi]);
    let result = harness.run_alias("kmax", ["--json", "--foreground", "--goal", "tests pass"]);
    assert!(!result.status.success());
    assert!(
        format!("{}\n{}", result.stdout, result.stderr)
            .contains("do not support --goal or --max-turns")
    );
    assert!(harness.invocations().is_empty());
}
