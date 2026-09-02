use neomax_core::tasks::{TaskStatus, TaskStore};

use crate::tasks;
use crate::tests::fixture;

#[test]
fn add_uses_project_and_updates_status_note_and_run() {
    let fixture = fixture();
    let args = vec![
        "add".into(),
        "--project=demo".into(),
        "--note".into(),
        "first note".into(),
        "ship".into(),
        "the".into(),
        "feature".into(),
    ];
    tasks::run(&fixture.context, &args).expect("add task");
    let store = TaskStore::new(&fixture.context.paths.tasks);
    let task = store.load().tasks.remove("t1").expect("task t1");
    assert_eq!(task.title, "ship the feature");
    assert_eq!(task.project.as_deref(), Some("demo"));
    assert_eq!(task.status, TaskStatus::Todo);
    assert_eq!(task.notes, vec!["first note".to_owned()]);

    tasks::run(&fixture.context, &["start".into(), "t1".into()]).expect("start task");
    tasks::run(
        &fixture.context,
        &["note".into(), "t1".into(), "second".into(), "note".into()],
    )
    .expect("note task");
    tasks::run(
        &fixture.context,
        &["link".into(), "t1".into(), "run-1".into()],
    )
    .expect("link task");
    let task = store.load().tasks.remove("t1").expect("updated task");
    assert_eq!(task.status, TaskStatus::Doing);
    assert_eq!(
        task.notes,
        vec!["first note".to_owned(), "second note".to_owned()]
    );
    assert_eq!(task.runs, vec!["run-1".to_owned()]);
}

#[test]
fn list_excludes_completed_tasks_unless_all_is_requested() {
    let fixture = fixture();
    tasks::run(&fixture.context, &["add".into(), "open task".into()]).expect("add open task");
    tasks::run(&fixture.context, &["add".into(), "done task".into()]).expect("add done task");
    tasks::run(&fixture.context, &["done".into(), "t2".into()]).expect("complete task");
    let store = TaskStore::new(&fixture.context.paths.tasks);
    assert_eq!(store.list(None, false).len(), 1);
    assert_eq!(store.list(None, true).len(), 2);
}

#[test]
fn invalid_status_is_rejected_without_mutating_the_task() {
    let fixture = fixture();
    tasks::run(&fixture.context, &["add".into(), "task".into()]).expect("add task");
    let error = tasks::run(
        &fixture.context,
        &["status".into(), "t1".into(), "unknown".into()],
    )
    .expect_err("invalid status should fail");
    assert!(error.to_string().contains("task status must be"));
    assert_eq!(
        TaskStore::new(&fixture.context.paths.tasks)
            .load()
            .tasks
            .get("t1")
            .expect("task")
            .status,
        TaskStatus::Todo
    );
}
