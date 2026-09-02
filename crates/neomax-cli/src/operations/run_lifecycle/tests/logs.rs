use neomax_core::runs::{RunStatus, RunStore};

use super::fixture::{fixture, run};
use crate::operations::run_lifecycle::{RunLifecycleCommand, RunLifecycleReport, execute};

#[test]
fn log_reads_only_bounded_neomax_log_roots_and_extracts_structured_events() {
    let fixture = fixture();
    let path = fixture.context.paths.logs.join("run.attempt1.jsonl");
    std::fs::write(
        &path,
        concat!(
            "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"hello\"},{\"type\":\"tool_use\",\"name\":\"status\",\"input\":{\"json\":true}}]}}\n",
            "{\"type\":\"result\",\"subtype\":\"success\",\"result\":\"done\"}\n"
        ),
    )
    .unwrap();
    let mut record = run("run", RunStatus::Done, fixture.context.cwd.clone());
    record.log = Some(path);
    RunStore::new(&fixture.context.paths.runs)
        .create(&record)
        .unwrap();
    let report = execute(
        RunLifecycleCommand::Log,
        &fixture.context,
        &["run".into()],
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::Log(report) = report else {
        panic!("expected log report")
    };
    assert_eq!(report.entries.len(), 3);
    assert!(!report.truncated);
}
