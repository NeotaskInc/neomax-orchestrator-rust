#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::{
    E2eHarness,
    assertions::{
        assert_authorization_error, assert_not_worker_tagged, assert_orchestrator_tools,
        assert_root_argv, assert_worker_argv, assert_worker_tools, pinned_orchestrator_alias,
    },
};

#[test]
fn root_and_worker_provider_argv_use_distinct_typed_launch_modes() {
    let root_stdin: &[u8] = if cfg!(windows) {
        b"root-stdio\r\n"
    } else {
        b"root-stdio\n"
    };
    for engine in Engine::ALL {
        let launcher = pinned_orchestrator_alias(engine);
        let root_harness = E2eHarness::new([engine]);
        let root_args: Vec<String> = if engine == Engine::Kimi {
            vec!["--json".into(), "--foreground".into()]
        } else {
            vec![
                "--json".into(),
                "--foreground".into(),
                "root fixture".into(),
            ]
        };
        let root_result = root_harness.run_alias_with_stdin(launcher, root_args, root_stdin);
        let root_report = root_result.json();
        assert_eq!(
            root_report["status"], "done",
            "root {engine}: {root_report}"
        );
        let root_invocation = root_harness
            .invocations()
            .pop()
            .unwrap_or_else(|| panic!("root {engine} did not invoke its provider"));
        assert_orchestrator_tools(&root_invocation, launcher);
        assert_root_argv(&root_invocation, engine, launcher);
        assert_not_worker_tagged(&root_invocation, launcher);
        assert_eq!(
            root_invocation.field("stdin_probe"),
            Some("root-stdio"),
            "root {launcher} did not preserve inherited interactive stdin"
        );
        root_harness.assert_hermetic_invocations();

        let worker_harness = E2eHarness::new([engine]);
        let worker_result = worker_harness.run_with_env(
            [
                "dispatch",
                "--json",
                "--foreground",
                "--brief",
                "--engine",
                engine.as_str(),
                "worker fixture",
            ],
            worker_harness.authorized_orchestrator_environment(),
        );
        let worker_report = worker_result.json();
        assert_eq!(
            worker_report["status"], "done",
            "worker {engine}: {worker_report}"
        );
        let worker_invocation = worker_harness
            .invocations()
            .pop()
            .unwrap_or_else(|| panic!("worker {engine} did not invoke its provider"));
        assert_worker_tools(&worker_invocation, &format!("worker {engine}"));
        assert_worker_argv(&worker_invocation, engine, &format!("worker {engine}"));
        assert_eq!(
            worker_invocation.field("worker"),
            Some("1"),
            "worker {engine} was not tagged NEOMAX_WORKER"
        );
        assert_eq!(
            worker_invocation.field("stdin_probe"),
            Some("<eof>"),
            "worker {engine} unexpectedly inherited interactive stdin"
        );
        worker_harness.assert_hermetic_invocations();
    }
}

#[test]
fn universal_auto_and_account_dispatch_require_worker_authorization() {
    let harness = E2eHarness::new([Engine::Opencode]);
    for selector in ["auto", "1"] {
        let result = harness.run(["--json", "--foreground", selector, "worker fixture"]);
        assert!(
            !result.status.success(),
            "unauthorized {selector} dispatch ran"
        );
        assert!(
            harness.invocations().is_empty(),
            "unauthorized {selector} spawned a provider"
        );
        let error = format!("{}\n{}", result.stdout, result.stderr);
        assert_authorization_error(&error, selector);
    }
}

#[test]
fn dry_run_worker_plan_is_read_only_and_does_not_require_worker_authorization() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run([
        "--dry-run",
        "--json",
        "--worker-dispatch",
        "--engine",
        "opencode",
        "worker plan fixture",
    ]);
    assert!(
        result.status.success(),
        "dry-run worker plan failed: status={} stdout={} stderr={}",
        result.status,
        result.stdout,
        result.stderr
    );
    let plan = result.json();
    assert_eq!(plan["provider_execution"], "disabled");
    assert_eq!(plan["worker_dispatch"], true);
    assert!(
        harness.invocations().is_empty(),
        "dry-run worker plan started a provider"
    );
    harness.assert_hermetic_invocations();
}

#[test]
fn authorized_worker_dispatch_uses_worker_policy_for_auto_and_account_forms() {
    for selector in ["auto", "1"] {
        let harness = E2eHarness::new([Engine::Opencode]);
        let result = harness.run_with_env(
            [
                "dispatch",
                "--json",
                "--foreground",
                "--brief",
                selector,
                "worker fixture",
            ],
            harness.authorized_orchestrator_environment(),
        );
        let report = result.json();
        assert_eq!(report["status"], "done", "selector {selector}: {report}");
        let invocation = harness
            .invocations()
            .pop()
            .unwrap_or_else(|| panic!("authorized {selector} dispatch did not invoke provider"));
        assert_worker_tools(&invocation, selector);
        assert_worker_argv(&invocation, Engine::Opencode, selector);
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn explicit_worker_dispatch_override_allows_a_worker_dispatch_without_session_role() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run_with_env(
        ["--json", "--foreground", "auto", "worker fixture"],
        [("NEOMAX_ALLOW_WORKER_DISPATCH", "1")],
    );
    let report = result.json();
    assert_eq!(report["status"], "done");
    let invocation = harness
        .invocations()
        .pop()
        .expect("worker-dispatch override did not invoke provider");
    assert_worker_tools(&invocation, "worker-dispatch override");
    assert_worker_argv(&invocation, Engine::Opencode, "worker-dispatch override");
    harness.assert_hermetic_invocations();
}

#[test]
fn canonical_dispatch_command_reaches_the_worker_execution_path() {
    let harness = E2eHarness::new([Engine::Opencode]);
    let result = harness.run_with_env(
        [
            "dispatch",
            "--json",
            "--foreground",
            "--engine",
            "opencode",
            "canonical worker fixture",
        ],
        [("NEOMAX_ALLOW_WORKER_DISPATCH", "1")],
    );
    let report = result.json();
    assert_eq!(report["status"], "done");
    let invocation = harness
        .invocations()
        .pop()
        .expect("canonical dispatch did not invoke the worker provider");
    assert_worker_tools(&invocation, "dispatch");
    assert_worker_argv(&invocation, Engine::Opencode, "dispatch");
    harness.assert_hermetic_invocations();
}
