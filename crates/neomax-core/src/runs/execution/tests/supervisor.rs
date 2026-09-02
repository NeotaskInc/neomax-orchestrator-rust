use std::time::Duration;

use crate::providers::{
    OrchestratorEnvironment, OrchestratorRequest, ProviderCommand, ProviderProfile,
};
use crate::runs::RunStatus;
use crate::{Engine, WorkerScope};

use super::super::{AttemptSupervisor, PreparedAttempt, QuotaRotation, SupervisorDirective};
use super::fixture::{FakeProvider, command, config, run};

#[test]
fn executes_and_parses_a_local_fixture_process() {
    let temp = tempfile::tempdir().unwrap();
    let prepared = PreparedAttempt::from_command(command(temp.path(), "printf ok"));
    let mut run = run(temp.path());
    let mut saved_pid = None;
    let outcome = AttemptSupervisor::new(&FakeProvider, config())
        .run(prepared, &mut run, temp.path(), false, |record| {
            saved_pid = record.worker_pid;
            Ok(())
        })
        .unwrap();
    assert_eq!(outcome.status, RunStatus::Done);
    assert!(saved_pid.is_some());
    assert_eq!(run.result_text.as_deref(), Some("complete"));
}

#[test]
fn runs_a_headless_bootstrap_before_resuming_the_interactive_command() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ProviderProfile {
        engine: Engine::Claude,
        account: "fixture".into(),
        path: temp.path().join("profile"),
        reserved: false,
    };
    let request = OrchestratorRequest::new(
        profile,
        temp.path(),
        temp.path(),
        OrchestratorEnvironment::new(WorkerScope::all(), "bootstrap-run"),
    );
    let prepared = PreparedAttempt::from_command(command(temp.path(), "printf main-command"))
        .with_bootstrap(command(temp.path(), "printf resume_hint"), request);
    let mut run = run(temp.path());
    let outcome = AttemptSupervisor::new(&FakeProvider, config())
        .run(prepared, &mut run, temp.path(), false, |_| Ok(()))
        .unwrap();

    assert_eq!(outcome.status, RunStatus::Done);
    assert_eq!(run.session.as_deref(), Some("session-bootstrap"));
    let output = std::fs::read_to_string(run.log.as_ref().unwrap()).unwrap();
    assert!(output.contains("resume_hint"));
    assert!(output.contains("ok-resumed-session-bootstrap"));
    assert!(!output.contains("main-command"));
}

#[test]
fn classifies_structured_and_stderr_rate_limits() {
    let temp = tempfile::tempdir().unwrap();
    for script in ["printf rate", "printf '429 quota exhausted' >&2; exit 1"] {
        let prepared = PreparedAttempt::from_command(command(temp.path(), script));
        let outcome = AttemptSupervisor::new(&FakeProvider, config())
            .run(prepared, &mut run(temp.path()), temp.path(), false, |_| {
                Ok(())
            })
            .unwrap();
        assert_eq!(outcome.status, RunStatus::Limit);
    }
}

#[test]
fn enforces_independent_stall_and_wall_deadlines() {
    let temp = tempfile::tempdir().unwrap();
    let prepared = PreparedAttempt::from_command(command(temp.path(), "sleep 2"));
    let mut stalled = config();
    stalled.stall_timeout = Some(Duration::from_millis(30));
    let outcome = AttemptSupervisor::new(&FakeProvider, stalled)
        .run(prepared, &mut run(temp.path()), temp.path(), false, |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(outcome.status, RunStatus::Stalled);

    let prepared = PreparedAttempt::from_command(command(
        temp.path(),
        "while true; do printf .; sleep 0.01; done",
    ));
    let mut timed = config();
    timed.wall_timeout = Some(Duration::from_millis(40));
    timed.stall_timeout = None;
    let outcome = AttemptSupervisor::new(&FakeProvider, timed)
        .run(prepared, &mut run(temp.path()), temp.path(), false, |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(outcome.status, RunStatus::Timeout);
}

#[test]
fn rotates_a_live_worker_when_the_monitor_reaches_the_hard_wall() {
    let temp = tempfile::tempdir().unwrap();
    let prepared = PreparedAttempt::from_command(command(
        temp.path(),
        "while true; do printf .; sleep 0.01; done",
    ));
    let mut checks = 0;
    let outcome = AttemptSupervisor::new(&FakeProvider, config())
        .run_monitored(
            prepared,
            &mut run(temp.path()),
            temp.path(),
            false,
            |_| Ok(()),
            || {
                checks += 1;
                Ok(if checks >= 2 {
                    SupervisorDirective::Rotate(QuotaRotation {
                        reason: "weekly 99%".into(),
                        resets_at: Some(500.0),
                        limit_window: Some("weekly".into()),
                    })
                } else {
                    SupervisorDirective::Continue
                })
            },
        )
        .unwrap();
    assert_eq!(outcome.status, RunStatus::Limit);
    assert_eq!(outcome.parsed.resets_at, Some(500.0));
    assert_eq!(outcome.parsed.limit_window.as_deref(), Some("weekly"));
    assert!(
        outcome
            .parsed
            .errors
            .iter()
            .any(|item| item == "weekly 99%")
    );
}

#[test]
fn aborts_a_live_worker_on_a_control_directive() {
    let temp = tempfile::tempdir().unwrap();
    let prepared =
        PreparedAttempt::from_command(ProviderCommand::new("sleep", temp.path()).arg("2"));
    let outcome = AttemptSupervisor::new(&FakeProvider, config())
        .run_monitored(
            prepared,
            &mut run(temp.path()),
            temp.path(),
            false,
            |_| Ok(()),
            || Ok(SupervisorDirective::Abort),
        )
        .unwrap();
    assert_eq!(outcome.status, RunStatus::Aborted);
}
