#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::E2eHarness;

#[test]
fn managed_workers_use_an_isolated_worktree_by_default_and_honor_the_override() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run_with_env(
        [
            "dispatch",
            "--json",
            "--foreground",
            "--brief",
            "--engine",
            "opencode",
            "isolated fixture",
        ],
        harness.authorized_orchestrator_environment(),
    );
    let report = result.json();
    let id = report["run_id"].as_str().expect("isolated run id");
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(harness.run_path(id)).unwrap()).unwrap();
    assert!(record["worktree"].is_string());
    assert!(record["repo"].is_string());
    harness.assert_hermetic_invocations();

    let no_worktree = E2eHarness::new([Engine::Opencode]);
    let mut no_worktree_environment = no_worktree.authorized_orchestrator_environment();
    no_worktree_environment.push(("NEOMAX_NO_WORKTREE".into(), "1".into()));
    let result = no_worktree.run_with_env(
        [
            "dispatch",
            "--json",
            "--foreground",
            "--brief",
            "--engine",
            "opencode",
            "shared fixture",
        ],
        no_worktree_environment,
    );
    let report = result.json();
    let id = report["run_id"].as_str().expect("shared run id");
    let record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(no_worktree.run_path(id)).expect("shared run record"),
    )
    .unwrap();
    assert!(record["worktree"].is_null());
    assert!(record["repo"].is_null());
    no_worktree.assert_hermetic_invocations();
}
