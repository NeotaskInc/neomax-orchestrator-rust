#[path = "e2e_support/mod.rs"]
mod support;

use std::fs;

use neomax_core::Engine;

use support::{
    E2eHarness,
    assertions::{assert_not_worker_tagged, assert_orchestrator_tools, assert_root_argv},
};

#[test]
fn cmax_numeric_selection_remains_a_main_claude_launch() {
    let mut harness = E2eHarness::new([Engine::Claude]);
    let account_two = harness.add_profile(Engine::Claude, 2);
    let result = harness.run_alias(
        "cmax",
        ["--json", "--foreground", "2", "account-pinned fixture"],
    );
    let report = result.json();
    assert_eq!(report["status"], "done");
    assert_eq!(report["engine"], "claude");
    let account_name = account_two
        .file_name()
        .and_then(|name| name.to_str())
        .expect("account 2 name");
    assert_eq!(report["account"], account_name);
    let invocation = harness
        .invocations()
        .pop()
        .expect("cmax numeric launch did not invoke Claude");
    let profile_path = fs::canonicalize(&account_two)
        .expect("canonical account 2 profile")
        .to_string_lossy()
        .into_owned();
    assert_eq!(invocation.field("profile"), Some(profile_path.as_str()));
    assert_orchestrator_tools(&invocation, "cmax account 2");
    assert_root_argv(&invocation, Engine::Claude, "cmax account 2");
    assert_not_worker_tagged(&invocation, "cmax account 2");
    harness.assert_hermetic_invocations();
}

#[test]
fn cmax_numeric_login_starts_an_unconfigured_claude_profile() {
    let harness = E2eHarness::new([Engine::Claude]);
    let result = harness.run_alias("cmax", ["--json", "--foreground", "2", "/login"]);
    let report = result.json();
    assert_eq!(report["status"], "done");
    assert_eq!(report["engine"], "claude");
    assert_eq!(report["account"], ".claude-acct2");
    let invocation = harness
        .invocations()
        .pop()
        .expect("cmax numeric login did not invoke Claude");
    assert_eq!(
        std::path::Path::new(invocation.field("profile").expect("profile"))
            .file_name()
            .and_then(|value| value.to_str()),
        Some(".claude-acct2")
    );
    assert!(invocation.has_arg("/login"));
    harness.assert_hermetic_invocations();
}

#[test]
fn cmax_orchestrator_uses_the_dedicated_claude_profile_without_reserved_mode() {
    let harness = E2eHarness::new([Engine::Claude]);
    let result = harness.run_alias("cmax", ["--json", "--foreground", "orchestrator"]);
    let report = result.json();
    assert_eq!(report["status"], "done");
    assert_eq!(report["engine"], "claude");
    assert_eq!(report["account"], "orch");
    let invocation = harness
        .invocations()
        .pop()
        .expect("cmax orchestrator did not invoke Claude");
    assert_eq!(
        std::path::Path::new(invocation.field("profile").expect("profile"))
            .file_name()
            .and_then(|value| value.to_str()),
        Some(".claude-orch")
    );
    assert_eq!(invocation.field("role"), Some("claude"));
    assert_eq!(invocation.field("mode"), Some("orchestrator"));
    assert_eq!(invocation.field("tool_policy"), Some("orchestrator"));
    harness.assert_hermetic_invocations();
}

#[test]
fn cmax_pinned_profile_creation_uses_configured_profile_and_orchestrator_roots() {
    let harness = E2eHarness::new([Engine::Claude]);
    let first = harness.home.join("configured/claude-one");
    let second = harness.home.join("configured/claude-two");
    let orchestrator = harness.home.join("configured/claude-orchestrator");
    let profile_roots = std::env::join_paths([&first, &second])
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let orchestrator_root = orchestrator.to_string_lossy().into_owned();
    let environment = [
        ("NEOMAX_PROFILES", profile_roots.as_str()),
        ("NEOMAX_CLAUDE_ORCH", orchestrator_root.as_str()),
    ];

    let numeric = harness.run_alias_with_env(
        "cmax",
        ["--json", "--foreground", "2", "/login"],
        environment.iter().copied(),
    );
    let numeric_report = numeric.json();
    assert_eq!(numeric_report["account"], "claude-two");
    let numeric_invocation = harness
        .invocations()
        .pop()
        .expect("configured numeric profile did not invoke Claude");
    let canonical_second = fs::canonicalize(&second)
        .expect("canonical configured Claude profile")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        numeric_invocation.field("profile"),
        Some(canonical_second.as_str())
    );
    assert!(!harness.home.join(".claude-acct2").exists());

    let orchestrator_result = harness.run_alias_with_env(
        "cmax",
        ["--json", "--foreground", "orchestrator"],
        environment.iter().copied(),
    );
    let orchestrator_report = orchestrator_result.json();
    assert_eq!(orchestrator_report["account"], "claude-orchestrator");
    let orchestrator_invocation = harness
        .invocations()
        .pop()
        .expect("configured orchestrator profile did not invoke Claude");
    let canonical_orchestrator = fs::canonicalize(&orchestrator)
        .expect("canonical configured Claude orchestrator profile")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        orchestrator_invocation.field("profile"),
        Some(canonical_orchestrator.as_str())
    );
    assert!(!harness.home.join(".claude-orch").exists());
    harness.assert_hermetic_invocations();
}

#[test]
fn helper_numeric_selection_remains_login_for_every_account_alias() {
    for (alias, engine) in [
        ("cdx", Engine::Codex),
        ("ocx", Engine::Opencode),
        ("kmx", Engine::Kimi),
        ("gmx", Engine::Grok),
    ] {
        let harness = E2eHarness::new([engine]);
        let result = harness.run_alias(alias, ["--json", "1"]);
        let report = result.json();
        assert_eq!(report["operation"], "login", "alias {alias}");
        assert_eq!(report["account"], "1", "alias {alias}");
        let invocation = harness
            .invocations()
            .pop()
            .unwrap_or_else(|| panic!("{alias} numeric helper did not invoke provider"));
        assert!(invocation.has_arg("login"), "alias {alias} was not login");
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn gmx_json_numeric_login_uses_the_noninteractive_oauth_default() {
    let harness = E2eHarness::new([Engine::Grok]);
    let result = harness.run_alias("gmx", ["--json", "1"]);
    let report = result.json();
    assert_eq!(report["operation"], "login");
    assert_eq!(report["account"], "1");
    assert!(
        result.stderr.trim().is_empty(),
        "JSON account login must not emit an interactive auth selector: {}",
        result.stderr
    );
    harness.assert_hermetic_invocations();
}

#[test]
fn account_helper_aliases_use_only_the_selected_fixture_profile() {
    for (alias, engine) in [
        ("cdx", Engine::Codex),
        ("ocx", Engine::Opencode),
        ("kmx", Engine::Kimi),
        ("gmx", Engine::Grok),
    ] {
        let harness = E2eHarness::new([engine]);
        let result = harness.run_alias(alias, ["--json", "login", "1"]);
        let report = result.json();
        assert_eq!(report["engine"], engine.as_str(), "alias {alias}");
        assert_eq!(report["account"], "1", "alias {alias}");
        assert_eq!(report["success"], true, "alias {alias}");
        assert_eq!(harness.invocations().len(), 1, "alias {alias}");
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn canonical_account_manifest_commands_route_to_flat_operations() {
    let harness = E2eHarness::new([Engine::Opencode]);

    let status = harness.run(["account", "status", "--json"]);
    let status_report = status.json();
    assert_eq!(status_report["engines"]["opencode"]["engine"], "opencode");

    let paused = harness.run(["account", "pause", "all", "--engine", "opencode", "--json"]);
    let paused_report = paused.json();
    assert_eq!(paused_report.as_array().unwrap().len(), 1);
    assert_eq!(paused_report[0]["paused"], true);

    let unpaused = harness.run([
        "account", "unpause", "all", "--engine", "opencode", "--json",
    ]);
    let unpaused_report = unpaused.json();
    assert_eq!(unpaused_report.as_array().unwrap().len(), 1);
    assert_eq!(unpaused_report[0]["paused"], false);

    let rotated = harness.run(["account", "rotate", "--dry-run", "--json"]);
    let rotated_report = rotated.json();
    assert_eq!(rotated_report["status"], "no-op");
    assert!(
        harness.invocations().is_empty(),
        "account control commands unexpectedly started a provider"
    );
}
