use super::*;
use serde_json::Value;

#[test]
fn adds_updates_links_filters_and_removes_tasks() {
    let temp = tempfile::tempdir().unwrap();
    let store = TaskStore::new(temp.path().join("tasks.json"));
    let task = store
        .add(
            "Ship feature",
            Some("product".into()),
            TaskStatus::Todo,
            Some("start here".into()),
            100,
        )
        .unwrap();
    assert_eq!(task.id, "t1");
    let updated = store
        .update(
            &task.id,
            TaskPatch {
                status: Some(TaskStatus::Done),
                note: Some("verified".into()),
                run_id: Some("run-1".into()),
                ..TaskPatch::default()
            },
            200,
        )
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, TaskStatus::Done);
    assert_eq!(updated.notes, ["start here", "verified"]);
    assert_eq!(updated.runs, ["run-1"]);
    assert!(store.list(Some("product"), false).is_empty());
    assert_eq!(store.list(Some("product"), true).len(), 1);
    assert!(store.remove("t1").unwrap().is_some());
    assert!(store.load().tasks.is_empty());
}

#[test]
fn concurrent_adds_use_unique_monotonic_ids() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tasks.json");
    std::thread::scope(|scope| {
        for index in 0..16 {
            let path = &path;
            scope.spawn(move || {
                TaskStore::new(path)
                    .add(
                        &format!("Task {index}"),
                        None,
                        TaskStatus::Todo,
                        None,
                        index,
                    )
                    .unwrap();
            });
        }
    });
    let state = TaskStore::new(path).load();
    assert_eq!(state.seq, 16);
    assert_eq!(state.tasks.len(), 16);
    assert!((1..=16).all(|index| state.tasks.contains_key(&format!("t{index}"))));
}

#[test]
fn rejects_blank_titles_and_deduplicates_run_links() {
    let temp = tempfile::tempdir().unwrap();
    let store = TaskStore::new(temp.path().join("tasks.json"));
    assert!(store.add(" ", None, TaskStatus::Todo, None, 1).is_err());
    let task = store.add("Work", None, TaskStatus::Todo, None, 1).unwrap();
    for now in [2, 3] {
        store
            .update(
                &task.id,
                TaskPatch {
                    run_id: Some("run".into()),
                    ..TaskPatch::default()
                },
                now,
            )
            .unwrap();
    }
    assert_eq!(store.load().tasks[&task.id].runs, ["run"]);
}

#[test]
fn mutations_refuse_to_replace_a_corrupt_backlog() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tasks.json");
    std::fs::write(&path, b"{").unwrap();
    let store = TaskStore::new(&path);
    assert!(store.add("work", None, TaskStatus::Todo, None, 1).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"{");
}

#[test]
fn preserves_unknown_status_and_registry_fields_across_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tasks.json");
    std::fs::write(
        &path,
        r#"{
          "seq": 7,
          "future_registry_field": {"enabled": true},
          "tasks": {
            "t1": {
              "id": "t1",
              "title": "Future task",
              "status": "waiting-on-review",
              "created": 1,
              "updated": 2,
              "future_task_field": {"preserve": true}
            },
            "t2": {
              "id": "t2",
              "title": "Known task",
              "status": "todo",
              "created": 3,
              "updated": 4
            }
          }
        }"#,
    )
    .unwrap();

    let store = TaskStore::new(&path);
    let loaded = store.try_load().unwrap();
    assert_eq!(loaded.tasks["t1"].status, TaskStatus::Unknown("waiting-on-review".into()));
    assert!(!loaded.tasks["t1"].status.is_known());
    assert_eq!(loaded.tasks["t1"].data["future_task_field"]["preserve"], true);
    assert_eq!(loaded.extra["future_registry_field"]["enabled"], true);

    store
        .update(
            "t2",
            TaskPatch {
                note: Some("changed without rewriting future state".into()),
                ..TaskPatch::default()
            },
            5,
        )
        .unwrap();

    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["seq"], 7);
    assert_eq!(saved["future_registry_field"]["enabled"], true);
    assert_eq!(saved["tasks"]["t1"]["status"], "waiting-on-review");
    assert_eq!(saved["tasks"]["t1"]["future_task_field"]["preserve"], true);
}

#[test]
fn isolates_malformed_tasks_and_keeps_them_during_unrelated_mutations() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tasks.json");
    std::fs::write(
        &path,
        r#"{
          "seq": 2,
          "tasks": {
            "t-good": {
              "id": "t-good",
              "title": "Recoverable",
              "status": "doing",
              "created": 1,
              "updated": 2
            },
            "t-bad": {
              "id": "t-bad",
              "title": 42,
              "status": "future-status",
              "created": 3,
              "updated": 4
            }
          }
        }"#,
    )
    .unwrap();

    let store = TaskStore::new(&path);
    let loaded = store.try_load().unwrap();
    assert_eq!(loaded.tasks.len(), 1);
    assert!(loaded.tasks.contains_key("t-good"));
    assert_eq!(loaded.invalid_tasks["t-bad"]["title"], 42);

    store
        .update(
            "t-good",
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..TaskPatch::default()
            },
            5,
        )
        .unwrap();

    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["tasks"]["t-good"]["status"], "done");
    assert_eq!(saved["tasks"]["t-bad"]["title"], 42);
    assert_eq!(store.try_load().unwrap().tasks["t-good"].status, TaskStatus::Done);
}

#[test]
fn new_tasks_do_not_reuse_quarantined_sequence_ids() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tasks.json");
    std::fs::write(
        &path,
        r#"{
          "seq": 1,
          "tasks": {
            "t2": {"id": "t2", "title": 42}
          }
        }"#,
    )
    .unwrap();

    let task = TaskStore::new(&path)
        .add("New task", None, TaskStatus::Todo, None, 5)
        .unwrap();
    assert_eq!(task.id, "t3");
    let saved: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved["tasks"]["t2"]["title"], 42);
    assert_eq!(saved["tasks"]["t3"]["title"], "New task");
}
