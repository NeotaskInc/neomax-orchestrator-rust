use super::super::{ProbeState, RunStatus, effective_status, in_inbox, worker_state};
use super::fixtures::{Probe, UnknownProbe, run};

#[test]
fn distinguishes_running_orphaned_and_interrupted_work() {
    assert_eq!(
        effective_status(
            &run(),
            &Probe {
                supervisor: true,
                worker: true,
            }
        ),
        RunStatus::Running
    );
    assert_eq!(
        effective_status(
            &run(),
            &Probe {
                supervisor: false,
                worker: true,
            }
        ),
        RunStatus::Orphaned
    );
    assert_eq!(
        effective_status(
            &run(),
            &Probe {
                supervisor: false,
                worker: false,
            }
        ),
        RunStatus::Interrupted
    );
}

#[test]
fn inbox_requires_a_terminal_unacknowledged_result() {
    let mut item = run();
    item.status = RunStatus::Done;
    assert!(in_inbox(
        &item,
        &Probe {
            supervisor: false,
            worker: false,
        }
    ));
    item.acknowledged = Some(true);
    assert!(!in_inbox(
        &item,
        &Probe {
            supervisor: false,
            worker: false,
        }
    ));
}

#[test]
fn probe_failure_is_unknown_and_does_not_look_like_an_interrupted_run() {
    assert_eq!(worker_state(&run(), &UnknownProbe), ProbeState::Unknown);
    assert_eq!(effective_status(&run(), &UnknownProbe), RunStatus::Unknown);
    assert!(!in_inbox(&run(), &UnknownProbe));
}
