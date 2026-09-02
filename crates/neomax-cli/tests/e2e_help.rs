#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::E2eHarness;

const ROOT_LAUNCHERS: [(&str, Engine); 6] = [
    ("neomax", Engine::Claude),
    ("cmax", Engine::Claude),
    ("cdxmax", Engine::Codex),
    ("ocmax", Engine::Opencode),
    ("kmax", Engine::Kimi),
    ("gmax", Engine::Grok),
];

const ACCOUNT_HELPERS: [&str; 4] = ["cdx", "ocx", "kmx", "gmx"];

#[test]
fn leading_help_and_version_aliases_remain_available_on_every_launcher() {
    for (launcher, engine) in ROOT_LAUNCHERS {
        let harness = E2eHarness::new([engine]);
        for flag in ["--help", "-h", "--version", "-V"] {
            let result = harness.run_alias(launcher, [flag]);
            assert!(
                result.status.success(),
                "{launcher} {flag} failed\nstdout:\n{}\nstderr:\n{}",
                result.stdout,
                result.stderr
            );
            assert!(
                !result.stdout.is_empty(),
                "{launcher} {flag} produced no output"
            );
        }
        assert!(
            harness.invocations().is_empty(),
            "{launcher} help/version path invoked a provider"
        );
    }

    for launcher in ACCOUNT_HELPERS {
        let harness = E2eHarness::new([Engine::Codex]);
        for flag in ["--help", "-h", "--version", "-V"] {
            let result = harness.run_alias(launcher, [flag]);
            assert!(
                result.status.success(),
                "{launcher} {flag} failed\nstdout:\n{}\nstderr:\n{}",
                result.stdout,
                result.stderr
            );
            assert!(
                !result.stdout.is_empty(),
                "{launcher} {flag} produced no output"
            );
        }
        assert!(
            harness.invocations().is_empty(),
            "{launcher} help/version path invoked a provider"
        );
    }
}

#[test]
fn delimiter_payload_help_and_version_are_forwarded_to_each_root_provider() {
    for (launcher, engine) in ROOT_LAUNCHERS {
        let harness = E2eHarness::new([engine]);
        let args = vec!["--json", "--foreground", "--", "--help", "-V"];
        let result = harness.run_alias(launcher, args);
        assert!(
            result.status.success(),
            "{launcher} delimiter payload failed\nstdout:\n{}\nstderr:\n{}",
            result.stdout,
            result.stderr
        );
        assert!(
            result.stdout.contains("\"status\"")
                || result.stdout.contains("launch plan")
                || result.stdout.contains("orchestrator"),
            "{launcher} appears to have taken the top-level help/version path:\n{}",
            result.stdout
        );
        let invocations = harness.invocations();
        assert!(
            !invocations.is_empty(),
            "{launcher} did not invoke its provider"
        );
        assert!(
            invocations
                .iter()
                .any(|invocation| invocation.args.iter().any(|arg| arg.contains("--help")))
        );
        assert!(
            invocations
                .iter()
                .any(|invocation| invocation.args.iter().any(|arg| arg.contains("-V"))),
            "{launcher} did not forward -V after --"
        );
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn delimiter_payload_is_not_reclassified_as_an_account_helper_command() {
    for launcher in ACCOUNT_HELPERS {
        let harness = E2eHarness::new([Engine::Codex]);
        let result = harness.run_alias(launcher, ["--", "--help"]);
        assert!(
            !result.status.success(),
            "{launcher} accepted an invalid helper payload"
        );
        assert!(
            !result.stdout.contains("Usage:") && !result.stdout.contains("account helper"),
            "{launcher} treated delimiter payload as top-level help:\n{}",
            result.stdout
        );
        assert!(harness.invocations().is_empty());
    }
}
