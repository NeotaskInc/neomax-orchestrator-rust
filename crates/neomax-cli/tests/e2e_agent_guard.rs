#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;
use neomax_core::agent_tools::{
    ManifestStore, NEOMAX_BIN_ENV, NEOMAX_TOOL_DEPTH_ENV, NEOMAX_TOOL_INSTRUCTION_ENV,
    NEOMAX_TOOL_MANIFEST_ENV, NEOMAX_TOOL_MAX_DEPTH_ENV, NEOMAX_TOOL_POLICY_ENV, TOOL_INSTRUCTION,
};

use support::E2eHarness;

fn agent_environment(harness: &E2eHarness) -> Vec<(String, String)> {
    agent_environment_at(harness, "1", "4")
}

fn agent_environment_at(
    harness: &E2eHarness,
    depth: &str,
    max_depth: &str,
) -> Vec<(String, String)> {
    let manifest = harness.state.join("agent-tools/manifest.json");
    ManifestStore::new(&manifest)
        .write_canonical()
        .expect("canonical agent manifest");
    vec![
        (NEOMAX_BIN_ENV.into(), env!("CARGO_BIN_EXE_neomax").into()),
        (
            NEOMAX_TOOL_MANIFEST_ENV.into(),
            manifest.to_string_lossy().into_owned(),
        ),
        (NEOMAX_TOOL_POLICY_ENV.into(), "worker".into()),
        (NEOMAX_TOOL_DEPTH_ENV.into(), depth.into()),
        (NEOMAX_TOOL_MAX_DEPTH_ENV.into(), max_depth.into()),
        (NEOMAX_TOOL_INSTRUCTION_ENV.into(), TOOL_INSTRUCTION.into()),
        ("NEOMAX_ROLE".into(), "opencode".into()),
        ("NEOMAX_WORKER".into(), "1".into()),
    ]
}

fn orchestrator_environment(harness: &E2eHarness) -> Vec<(String, String)> {
    orchestrator_environment_at(harness, "0", "4")
}

fn orchestrator_environment_at(
    harness: &E2eHarness,
    depth: &str,
    max_depth: &str,
) -> Vec<(String, String)> {
    let mut environment = agent_environment_at(harness, depth, max_depth);
    environment.retain(|(name, _)| name != NEOMAX_TOOL_POLICY_ENV && name != "NEOMAX_WORKER");
    environment.push((NEOMAX_TOOL_POLICY_ENV.into(), "orchestrator".into()));
    environment.push(("NEOMAX_ORCHESTRATOR".into(), "1".into()));
    environment
}

fn error_text(result: &support::process::CommandResult) -> String {
    format!("{}\n{}", result.stdout, result.stderr)
}

#[test]
fn malformed_agent_calls_fail_closed_without_provider_execution() {
    for args in [
        Vec::<&str>::new(),
        vec!["fix", "the", "build"],
        vec!["--engine", "opencode", "fix", "the", "build"],
        vec!["--", "status"],
        vec!["--engine", "opencode", "--", "status"],
        vec!["--", "--worker-dispatch", "fixture task"],
        vec!["fix", "--worker-dispatch", "fixture task"],
    ] {
        let harness = E2eHarness::new([Engine::Opencode]);
        let label = format!("{args:?}");
        let result = harness.run_with_env(args.clone(), agent_environment(&harness));
        assert!(
            !result.status.success(),
            "malformed agent call ran: {label}"
        );
        let error = error_text(&result);
        assert!(
            error.contains("canonical Neomax tool command")
                || error.contains("unknown agent command"),
            "malformed agent call returned an unrelated error: {error}"
        );
        assert!(
            harness.invocations().is_empty(),
            "malformed agent call invoked a provider: {label}"
        );
    }
}

#[test]
fn canonical_agent_commands_after_global_options_do_not_fall_into_root_launch() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run_with_env(
        ["--json", "--engine", "opencode", "status"],
        agent_environment(&harness),
    );
    let report = result.json();
    assert!(
        report["summary"].is_object(),
        "status command returned no summary"
    );
    assert!(
        harness.invocations().is_empty(),
        "status after global options started a provider"
    );
}

#[test]
fn partial_agent_markers_fail_closed_before_a_root_launch() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let environment = agent_environment(&harness)
        .into_iter()
        .filter(|(name, _)| name != NEOMAX_BIN_ENV)
        .collect::<Vec<_>>();
    let result = harness.run_with_env(["fix", "the", "build"], environment);
    assert!(!result.status.success());
    assert!(
        error_text(&result).contains("NEOMAX_BIN is required"),
        "partial agent markers returned an unrelated error: {}",
        error_text(&result)
    );
    assert!(harness.invocations().is_empty());
}

#[test]
fn canonical_agent_dispatch_keeps_guarded_worker_semantics() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run_with_env(
        [
            "dispatch",
            "--dry-run",
            "--json",
            "--foreground",
            "--engine",
            "opencode",
            "fixture task",
        ],
        orchestrator_environment(&harness),
    );
    let report = result.json();
    assert_eq!(report["worker_dispatch"], true);
    assert!(
        harness.invocations().is_empty(),
        "dry-run agent dispatch invoked a provider"
    );
}

#[test]
fn canonical_external_agent_commands_cannot_bypass_recursion_limit() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run_with_env(
        [
            "dispatch",
            "--dry-run",
            "--json",
            "--foreground",
            "--engine",
            "opencode",
            "fixture task",
        ],
        orchestrator_environment_at(&harness, "4", "4"),
    );
    assert!(!result.status.success());
    assert!(
        error_text(&result).contains("exceed the configured tool recursion limit"),
        "recursion failure was not actionable: {}",
        error_text(&result)
    );
    assert!(harness.invocations().is_empty());
}

#[test]
fn worker_agent_dispatch_is_rejected_before_provider_execution() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run_with_env(
        ["dispatch", "--dry-run", "--json", "fixture task"],
        agent_environment(&harness),
    );
    assert!(!result.status.success());
    assert!(error_text(&result).contains("tool policy denies external command dispatch"));
    assert!(harness.invocations().is_empty());
}
