#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::{
    E2eHarness,
    assertions::{assert_plan_argv, assert_worker_tools, pinned_orchestrator_alias},
};

#[test]
fn guarded_worker_plan_uses_each_provider_boundary_in_the_current_checkout() {
    for engine in Engine::ALL {
        let harness = E2eHarness::new([engine]);
        let result = harness.run_with_env(
            [
                "dispatch",
                "--json",
                "--foreground",
                "--engine",
                engine.as_str(),
                "--plan",
                "inspect the fixture",
            ],
            [("NEOMAX_ALLOW_WORKER_DISPATCH", "1")],
        );
        let report = result.json();
        assert_eq!(report["status"], "done", "worker plan {engine}: {report}");
        assert_eq!(report["engine"], engine.as_str(), "worker plan {engine}");

        let mut invocations = harness.invocations();
        assert_eq!(invocations.len(), 1, "worker plan {engine} provider count");
        let invocation = invocations.pop().expect("worker plan provider invocation");
        assert_plan_argv(&invocation, engine, &format!("worker plan {engine}"));
        assert_worker_tools(&invocation, &format!("worker plan {engine}"));
        assert_eq!(invocation.field("worker"), Some("1"));

        let run_id = report["run_id"].as_str().expect("worker plan run id");
        let record: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(harness.run_path(run_id)).expect("worker plan run record"),
        )
        .unwrap();
        assert_eq!(record["plan_mode"], true, "worker plan {engine}");
        assert_eq!(record["workdir"], record["cwd"], "worker plan {engine}");
        assert!(
            record["worktree"].is_null(),
            "worker plan {engine} allocated a worktree"
        );
        assert!(
            record["repo"].is_null(),
            "worker plan {engine} recorded a repository"
        );
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn root_orchestrator_plan_is_rejected_before_any_provider_execution() {
    for engine in Engine::ALL {
        let harness = E2eHarness::new([engine]);
        let result = harness.run_alias(
            pinned_orchestrator_alias(engine),
            ["--json", "--foreground", "--plan"],
        );
        assert!(
            !result.status.success(),
            "{} accepted root --plan",
            pinned_orchestrator_alias(engine)
        );
        let error = format!("{}\n{}", result.stdout, result.stderr);
        assert!(
            error.contains("only valid for guarded worker dispatch"),
            "root {engine} --plan returned an unrelated error: {error}"
        );
        assert!(
            harness.invocations().is_empty(),
            "root {engine} --plan executed a provider"
        );
    }
}
