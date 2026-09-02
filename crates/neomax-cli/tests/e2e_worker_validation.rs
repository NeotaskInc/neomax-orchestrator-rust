#[path = "e2e_support/mod.rs"]
mod support;

use support::E2eHarness;

const ALLOW_WORKER: [(&str, &str); 1] = [("NEOMAX_ALLOW_WORKER_DISPATCH", "1")];

fn error_text(result: &support::process::CommandResult) -> String {
    format!("{}\n{}", result.stdout, result.stderr)
}

#[test]
fn guarded_dispatch_requires_a_nonblank_task_even_when_goal_is_present() {
    let harness = E2eHarness::new([neomax_core::Engine::Opencode]);
    let result = harness.run_with_env(
        [
            "dispatch",
            "--dry-run",
            "--json",
            "--foreground",
            "--engine",
            "opencode",
            "--goal",
            "objective only",
        ],
        ALLOW_WORKER,
    );
    assert!(!result.status.success());
    assert!(error_text(&result).contains("--goal is not a task"));
    assert!(harness.invocations().is_empty());
}

#[test]
fn root_without_a_task_remains_a_valid_interactive_plan() {
    let harness = E2eHarness::new([neomax_core::Engine::Opencode]);
    let report = harness
        .run(["--dry-run", "--json", "--engine", "opencode"])
        .json();
    assert_eq!(report["worker_dispatch"], false);
    assert!(report["initial_task"].is_null());
}

#[test]
fn goals_are_trimmed_and_rejected_after_the_unicode_cap() {
    let harness = E2eHarness::new([neomax_core::Engine::Opencode]);
    let trimmed = harness
        .run([
            "--dry-run",
            "--json",
            "--engine",
            "opencode",
            "--goal=  objective  ",
        ])
        .json();
    assert_eq!(trimmed["goal"], "objective");

    let too_long = format!("--goal={}", "é".repeat(4_001));
    let result = harness.run(vec![
        "--dry-run".into(),
        "--json".into(),
        "--engine".into(),
        "opencode".into(),
        too_long,
    ]);
    assert!(!result.status.success());
    assert!(error_text(&result).contains("Unicode characters"));
}

#[test]
fn ineffective_base_combinations_fail_closed_before_provider_execution() {
    for extra in ["--no-worktree", "--plan"] {
        let harness = E2eHarness::new([neomax_core::Engine::Opencode]);
        let result = harness.run_with_env(
            [
                "dispatch",
                "--dry-run",
                "--json",
                "--foreground",
                "--engine",
                "opencode",
                "--base",
                "main",
                extra,
                "inspect fixture",
            ],
            ALLOW_WORKER,
        );
        assert!(!result.status.success(), "accepted --base with {extra}");
        assert!(error_text(&result).contains("effective base"));
        assert!(harness.invocations().is_empty());
    }
}

#[test]
fn retired_delegate_forces_guarded_worker_mode_on_universal_and_pinned_launchers() {
    for (alias, task) in [
        ("neomax", "universal delegate task"),
        ("ocmax", "pinned delegate task"),
    ] {
        let harness = E2eHarness::new([neomax_core::Engine::Opencode]);
        let report = harness
            .run_alias_with_env(
                alias,
                [
                    "delegate",
                    "--dry-run",
                    "--json",
                    "--foreground",
                    "--engine",
                    "opencode",
                    task,
                ],
                ALLOW_WORKER,
            )
            .json();
        assert_eq!(report["worker_dispatch"], true, "launcher {alias}");
    }
}

#[test]
fn thin_brief_warning_is_advisory_brief_aware_and_silent_for_scoped_acceptance() {
    let thin = E2eHarness::new([neomax_core::Engine::Opencode]);
    let result = thin.run_with_env(
        [
            "dispatch",
            "--dry-run",
            "--json",
            "--foreground",
            "--engine",
            "opencode",
            "fix fixture",
        ],
        ALLOW_WORKER,
    );
    result.assert_success();
    assert_eq!(
        result.stderr.matches("worker task brief is thin").count(),
        1
    );

    let acknowledged = E2eHarness::new([neomax_core::Engine::Opencode]);
    let result = acknowledged.run_with_env(
        [
            "dispatch",
            "--dry-run",
            "--json",
            "--foreground",
            "--engine",
            "opencode",
            "--brief",
            "fix fixture",
        ],
        ALLOW_WORKER,
    );
    result.assert_success();
    assert!(!result.stderr.contains("worker task brief is thin"));

    let adequate = E2eHarness::new([neomax_core::Engine::Opencode]);
    let result = adequate.run_with_env(
        [
            "dispatch",
            "--dry-run",
            "--json",
            "--foreground",
            "--engine",
            "opencode",
            "Objective:",
            "fix fixture",
            "Scope:",
            "fixture.txt",
            "Acceptance:",
            "tests pass",
        ],
        ALLOW_WORKER,
    );
    result.assert_success();
    assert!(!result.stderr.contains("worker task brief is thin"));
}

#[test]
fn detached_parent_emits_the_thin_brief_warning_once() {
    let harness = E2eHarness::new([neomax_core::Engine::Opencode]);
    let result = harness.run_with_env(
        [
            "dispatch",
            "--json",
            "--detach",
            "--engine",
            "opencode",
            "fix fixture",
        ],
        ALLOW_WORKER,
    );
    result.assert_success();
    assert_eq!(
        result.stderr.matches("worker task brief is thin").count(),
        1
    );
}
