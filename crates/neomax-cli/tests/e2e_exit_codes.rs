#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::E2eHarness;

#[test]
fn launchers_report_syntax_failures_as_usage_errors_without_starting_a_provider() {
    for (launcher, engine) in [
        ("neomax", Engine::Claude),
        ("cmax", Engine::Claude),
        ("cdxmax", Engine::Codex),
        ("ocmax", Engine::Opencode),
        ("kmax", Engine::Kimi),
        ("gmax", Engine::Grok),
    ] {
        let harness = E2eHarness::new([engine]);
        let result = harness.run_alias(launcher, ["--workers"]);
        assert_eq!(
            result.status.code(),
            Some(2),
            "{launcher} returned the wrong code"
        );
        assert!(
            harness.invocations().is_empty(),
            "{launcher} started a provider"
        );
    }
}

#[test]
fn account_helpers_report_parse_failures_as_usage_errors_without_starting_a_provider() {
    for (helper, engine) in [
        ("cdx", Engine::Codex),
        ("ocx", Engine::Opencode),
        ("kmx", Engine::Kimi),
        ("gmx", Engine::Grok),
    ] {
        let harness = E2eHarness::new([engine]);
        let result = harness.run_alias(helper, ["login"]);
        assert_eq!(
            result.status.code(),
            Some(2),
            "{helper} returned the wrong code"
        );
        assert!(
            harness.invocations().is_empty(),
            "{helper} started a provider"
        );
    }
}

#[test]
fn accepted_input_failures_keep_the_runtime_exit_code() {
    let harness = E2eHarness::new([Engine::Codex]);
    let result = harness.run_alias("cdx", ["run", "99"]);
    assert_eq!(
        result.status.code(),
        Some(1),
        "runtime failure returned the wrong code"
    );
    assert!(harness.invocations().is_empty());
}
