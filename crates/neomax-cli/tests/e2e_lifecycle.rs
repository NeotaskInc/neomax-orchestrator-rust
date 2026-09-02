#[path = "e2e_support/mod.rs"]
mod support;

use std::fs;
use std::time::Duration;

use neomax_core::Engine;

use support::{
    E2eHarness,
    wait::{wait_for_exit, wait_for_run},
};

#[test]
fn status_resume_and_retry_operate_on_the_same_durable_run() {
    let mut harness = E2eHarness::new([Engine::Claude]);
    let profile = harness.profile(Engine::Claude, 0).to_path_buf();
    let retry_profile = harness.add_profile(Engine::Claude, 2);
    harness.seed_quota(Engine::Claude, &profile, 0.0);
    harness.seed_quota(Engine::Claude, &retry_profile, 0.0);
    let first = harness.run_with_env(
        [
            "--json",
            "--foreground",
            "--no-worktree",
            "auto",
            "durable fixture",
        ],
        [("NEOMAX_ALLOW_WORKER_DISPATCH", "1")],
    );
    let first_report = first.json();
    let id = first_report["run_id"].as_str().expect("run id").to_owned();
    assert_eq!(first_report["status"], "done");

    let status = harness.run(["status", "--json"]);
    let status_json = status.json();
    let status_run = status_json["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["id"] == id)
        .expect("run in status");
    assert_eq!(status_run["status"], "done");

    let resumed = harness.run(["resume", id.as_str(), "--json"]);
    let resumed_json = resumed.json();
    assert_eq!(resumed_json["kind"], "rerun");
    assert_eq!(resumed_json["data"]["id"], id);
    assert_eq!(resumed_json["data"]["status"], "done");
    let retried = harness.run(["retry", id.as_str(), "--json"]);
    let retried_json = retried.json();
    assert_eq!(retried_json["kind"], "rerun");
    assert_eq!(retried_json["data"]["id"], id);
    assert_eq!(retried_json["data"]["status"], "done");
    let record: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(harness.run_path(&id)).expect("durable run record"),
    )
    .unwrap();
    assert_eq!(record["id"], id);
    assert_eq!(record["status"], "done");
    assert_eq!(record["attempt"], 3);
    assert_eq!(harness.invocations().len(), 3);
    harness.assert_hermetic_invocations();
}

#[test]
fn codex_resume_archives_the_old_thread_and_accepts_the_new_thread() {
    let harness = E2eHarness::new([Engine::Codex]);
    let first = harness.run_with_env(
        [
            "--json",
            "--foreground",
            "--no-worktree",
            "--engine",
            "codex",
            "auto",
            "codex durable fixture",
        ],
        [("NEOMAX_ALLOW_WORKER_DISPATCH", "1")],
    );
    let first_report = first.json();
    let id = first_report["run_id"].as_str().expect("run id").to_owned();
    let first_record: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(harness.run_path(&id)).expect("initial durable run record"),
    )
    .unwrap();
    assert_eq!(first_record["session"], "session-codex-1");

    let resumed = harness.run(["resume", id.as_str(), "--json"]);
    assert_eq!(resumed.json()["data"]["status"], "done");
    let resumed_record: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(harness.run_path(&id)).expect("resumed durable run record"),
    )
    .unwrap();
    assert_eq!(resumed_record["session"], "session-codex-2");
    assert_eq!(resumed_record["resumed"], false);
    assert_eq!(
        resumed_record["session_history"][0]["session"],
        "session-codex-1"
    );
    assert_eq!(harness.invocations().len(), 2);
    harness.assert_hermetic_invocations();
}

#[test]
fn kill_marks_a_running_run_aborted_and_terminates_the_fake_provider() {
    let harness = E2eHarness::with_behavior([Engine::Codex], "sleep");
    let mut child = harness.spawn_with_env(
        ["--engine", "codex", "auto", "kill fixture"],
        [("NEOMAX_ALLOW_WORKER_DISPATCH", "1")],
    );
    let (id, running) = wait_for_run(&harness, |run| {
        run["status"] == "running" && run["worker_pid"].as_u64().is_some()
    });
    assert!(running["worker_pid"].as_u64().is_some());

    let killed = harness.run(["kill", id.as_str(), "--json"]);
    let killed_json = killed.json();
    assert_eq!(killed_json["kind"], "kill");
    assert_eq!(killed_json["data"]["id"], id);
    assert_eq!(killed_json["data"]["marked"], true);
    let _ = wait_for_exit(&mut child);

    let mut aborted = false;
    for _ in 0..400 {
        let record: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(harness.run_path(&id)).expect("run record after kill"),
        )
        .unwrap();
        if record["status"] == "aborted" && record["killed"] == true {
            aborted = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(aborted, "killed run did not remain aborted");
    harness.assert_hermetic_invocations();
}
