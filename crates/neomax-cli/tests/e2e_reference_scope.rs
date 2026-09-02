#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::E2eHarness;

#[test]
fn inherited_fleet_restricts_dynamic_selection_and_explicit_workers() {
    let harness = E2eHarness::new([Engine::Claude, Engine::Opencode]);
    let result = harness.run_with_env(
        ["--json", "--foreground", "fixture task"],
        [("NEOMAX_FLEET", "opencode")],
    );
    let report = result.json();
    assert_eq!(report["engine"], "opencode");
    assert_eq!(report["worker_scope"], "opencode");
    harness.assert_hermetic_invocations();

    let rejected = harness.run_with_env(
        ["--dry-run", "--json", "--workers", "claude"],
        [("NEOMAX_FLEET", "opencode")],
    );
    assert!(!rejected.status.success());
    assert!(format!("{}\n{}", rejected.stdout, rejected.stderr).contains("scope is empty"));

    let worker_rejected = E2eHarness::new([Engine::Claude, Engine::Opencode]);
    let result = worker_rejected.run_with_env(
        [
            "--json",
            "--foreground",
            "--engine",
            "claude",
            "auto",
            "worker fixture",
        ],
        [("NEOMAX_WORKER", "1"), ("NEOMAX_FLEET", "opencode")],
    );
    assert!(!result.status.success());
    assert!(worker_rejected.invocations().is_empty());
}
