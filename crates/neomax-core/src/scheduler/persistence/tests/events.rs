use std::sync::Arc;

use chrono::{Datelike, Local, TimeZone, Utc};

use super::super::{PlanEvent, PlanEventStore};

#[test]
fn partitions_events_by_the_machine_local_day() {
    let temp = tempfile::tempdir().unwrap();
    let store = PlanEventStore::new(temp.path());
    let first_day = Utc.with_ymd_and_hms(2026, 8, 23, 12, 0, 0).unwrap();
    let second_day = Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap();
    store
        .append(&PlanEvent::new("batch-1", "started", 1).unwrap(), first_day)
        .unwrap();
    store
        .append(
            &PlanEvent::new("batch-1", "finished", 2).unwrap(),
            second_day,
        )
        .unwrap();
    let first_day = first_day.with_timezone(&Local).date_naive();
    let second_day = second_day.with_timezone(&Local).date_naive();
    assert!(
        temp.path()
            .join(format!(
                "scheduler/{:04}-{:02}-{:02}.jsonl",
                first_day.year(),
                first_day.month(),
                first_day.day()
            ))
            .exists()
    );
    assert!(
        temp.path()
            .join(format!(
                "scheduler/{:04}-{:02}-{:02}.jsonl",
                second_day.year(),
                second_day.month(),
                second_day.day()
            ))
            .exists()
    );
    assert_eq!(store.read(Some("batch-1"), 0).unwrap().len(), 2);
}

#[test]
fn concurrent_appends_are_locked_and_keep_each_json_line_intact() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(PlanEventStore::new(temp.path()));
    let at = Utc.with_ymd_and_hms(2026, 8, 23, 12, 0, 0).unwrap();
    std::thread::scope(|scope| {
        for index in 0..16 {
            let store = Arc::clone(&store);
            scope.spawn(move || {
                let mut event = PlanEvent::new("batch-1", "part", index).unwrap();
                event.extra.insert("index".into(), index.into());
                store.append(&event, at).unwrap();
            });
        }
    });
    let events = store.read(Some("batch-1"), 0).unwrap();
    assert_eq!(events.len(), 16);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.extra.contains_key("index"))
            .count(),
        16
    );
}

#[test]
fn scheduler_reads_legacy_root_without_consuming_run_events() {
    let temp = tempfile::tempdir().unwrap();
    let scheduler = temp.path().join("events/scheduler");
    let legacy = temp.path().join("events");
    std::fs::create_dir_all(&scheduler).unwrap();
    std::fs::write(
        legacy.join("2026-08-23.jsonl"),
        concat!(
            "{\"ts\":1,\"run\":\"run-1\",\"event\":\"finished\"}\n",
            "{\"ts\":1,\"plan_id\":\"plan-1\",\"event\":\"started\"}\n",
        ),
    )
    .unwrap();
    let store = PlanEventStore::with_legacy_directory(&scheduler, &legacy);
    let events = store.read(Some("plan-1"), 0).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].plan_id, "plan-1");
}

#[test]
fn malformed_event_lines_fail_with_the_source_path() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("scheduler")).unwrap();
    std::fs::write(temp.path().join("scheduler/2026-08-23.jsonl"), b"{\n{}\n").unwrap();
    let view = PlanEventStore::new(temp.path())
        .read_with_diagnostics(None, 0)
        .unwrap();
    assert!(view.events.is_empty());
    assert!(view.diagnostics[0].path.ends_with("2026-08-23.jsonl"));
    assert!(
        view.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("plan_id"))
    );
}

#[test]
fn oversized_event_files_fail_closed_before_json_parsing() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("scheduler")).unwrap();
    std::fs::write(
        temp.path().join("scheduler/2026-08-23.jsonl"),
        vec![b'x'; super::super::events::MAX_EVENT_FILE_BYTES + 1],
    )
    .unwrap();

    let view = PlanEventStore::new(temp.path())
        .read_with_diagnostics(None, 0)
        .unwrap();
    assert!(view.events.is_empty());
    assert!(view.diagnostics[0].message.contains("32"));
}

#[test]
fn invalid_utf8_event_lines_are_reported_as_corrupt_state() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("scheduler")).unwrap();
    std::fs::write(
        temp.path().join("scheduler/2026-08-23.jsonl"),
        [b'{', 0xff, b'}'],
    )
    .unwrap();

    let view = PlanEventStore::new(temp.path())
        .read_with_diagnostics(None, 0)
        .unwrap();
    assert!(view.events.is_empty());
    assert!(view.diagnostics[0].message.contains("UTF-8"));
}

#[test]
fn oversized_event_lines_fail_before_json_parsing() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("scheduler")).unwrap();
    std::fs::write(
        temp.path().join("scheduler/2026-08-23.jsonl"),
        vec![b'x'; super::super::events::MAX_EVENT_LINE_BYTES + 1],
    )
    .unwrap();

    let view = PlanEventStore::new(temp.path())
        .read_with_diagnostics(None, 0)
        .unwrap();
    assert!(view.events.is_empty());
    assert!(view.diagnostics[0].message.contains("line 1"));
}
