#[path = "e2e_support/mod.rs"]
mod support;

use std::fs;

use neomax_core::Engine;

use support::E2eHarness;

#[test]
fn fixed_worker_run_id_and_tag_survive_execution_and_lifecycle_reruns() {
    let mut harness = E2eHarness::new([Engine::Opencode]);
    let second_profile = harness.add_profile(Engine::Opencode, 2);
    let third_profile = harness.add_profile(Engine::Opencode, 3);
    harness.seed_quota(Engine::Opencode, harness.profile(Engine::Opencode, 0), 0.0);
    harness.seed_quota(Engine::Opencode, &second_profile, 0.0);
    harness.seed_quota(Engine::Opencode, &third_profile, 0.0);
    let first = harness.run_with_env(
        [
            "--json",
            "--foreground",
            "--run-id",
            "fixture-run",
            "--tag",
            "plan=fixture",
            "--no-worktree",
            "auto",
            "metadata fixture",
        ],
        [("NEOMAX_ALLOW_WORKER_DISPATCH", "1")],
    );
    let report = first.json();
    assert_eq!(report["run_id"], "fixture-run");
    assert_eq!(report["status"], "done");

    let record: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(harness.run_path("fixture-run")).expect("durable run record"),
    )
    .unwrap();
    assert_eq!(record["id"], "fixture-run");
    assert_eq!(record["tag"], "plan=fixture");

    let status = harness.run(["status", "--json"]);
    let status_json = status.json();
    let status_run = status_json["data"]["runs"]
        .as_array()
        .or_else(|| status_json["runs"].as_array())
        .and_then(|runs| runs.iter().find(|run| run["id"] == "fixture-run"))
        .expect("tagged run in status");
    assert_eq!(status_run["tag"], "plan=fixture");

    for command in ["resume", "retry"] {
        let rerun = harness.run([command, "fixture-run", "--json"]);
        let rerun_json = rerun.json();
        assert_eq!(rerun_json["data"]["id"], "fixture-run");
        let record: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(harness.run_path("fixture-run")).expect("rerun record"),
        )
        .unwrap();
        assert_eq!(record["tag"], "plan=fixture");
    }
    harness.assert_hermetic_invocations();
}
