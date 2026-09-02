use neomax_core::runs::{RunStatus, RunStore};

use super::fixture::{FakeProcess, fixture, run};
use crate::operations::run_lifecycle::{
    RunLifecycleCommand, RunLifecycleReport, execute_with_process,
};

#[test]
fn list_and_status_use_effective_process_state_and_inbox_markers() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let mut active = run("active", RunStatus::Running, fixture.context.cwd.clone());
    active.supervisor_pid = Some(41);
    let mut orphan = run("orphan", RunStatus::Running, fixture.context.cwd.clone());
    orphan.worker_pid = Some(42);
    let done = run("done", RunStatus::Done, fixture.context.cwd.clone());
    store.create(&active).unwrap();
    store.create(&orphan).unwrap();
    store.create(&done).unwrap();
    let process = FakeProcess {
        supervisors: std::sync::Arc::new(std::sync::Mutex::new(vec![41])),
        workers: std::sync::Arc::new(std::sync::Mutex::new(vec![42])),
        ..FakeProcess::default()
    };
    let report = execute_with_process(
        RunLifecycleCommand::List,
        &fixture.context,
        &[],
        &process,
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::List(report) = report else {
        panic!("expected list report")
    };
    assert_eq!(report.runs.len(), 3);
    assert_eq!(report.runs[0].status, "running");
    assert_eq!(report.orphaned, 1);

    let report = execute_with_process(
        RunLifecycleCommand::Status,
        &fixture.context,
        &["--status".into(), "orphaned".into()],
        &process,
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::Status(report) = report else {
        panic!("expected status report")
    };
    assert_eq!(report.runs.len(), 1);
    assert_eq!(report.runs[0].id, "orphan");
}
