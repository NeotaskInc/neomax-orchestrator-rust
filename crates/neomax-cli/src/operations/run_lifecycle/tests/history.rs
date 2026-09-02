use neomax_core::runs::{HistoryStore, RunStatus, RunStore};

use super::fixture::{fixture, run};
use crate::operations::run_lifecycle::{RunLifecycleCommand, RunLifecycleReport, execute};

#[test]
fn history_reads_the_permanent_archive_and_can_retrieve_archived_logs() {
    let fixture = fixture();
    let path = fixture.context.paths.logs.join("archived.attempt1.jsonl");
    std::fs::write(&path, "{\"type\":\"result\",\"result\":\"archived\"}\n").unwrap();
    let mut record = run("archived", RunStatus::Done, fixture.context.cwd.clone());
    record.log = Some(path);
    RunStore::new(&fixture.context.paths.runs)
        .create(&record)
        .unwrap();
    let history = HistoryStore::new(
        &fixture.context.paths.history_db,
        &fixture.context.paths.logs,
        &fixture.context.paths.history_logs,
        &fixture.context.paths.history_pending,
    );
    history.archive(&record, None, 10).unwrap();
    std::fs::remove_file(RunStore::new(&fixture.context.paths.runs).path("archived")).unwrap();
    let log_report = execute(
        RunLifecycleCommand::Log,
        &fixture.context,
        &["archived".into()],
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::Log(log_report) = log_report else {
        panic!("expected archived log report")
    };
    assert_eq!(log_report.entries.len(), 1);
    let report = execute(
        RunLifecycleCommand::History,
        &fixture.context,
        &["archived".into(), "--log".into()],
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::History(report) = report else {
        panic!("expected history report")
    };
    assert_eq!(report.detail.unwrap().run.id, "archived");
    assert_eq!(report.log.unwrap().entries.len(), 1);
}
