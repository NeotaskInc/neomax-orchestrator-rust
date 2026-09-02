use std::collections::BTreeMap;
use std::fs;

use neomax_core::projects::Project;
use neomax_core::queue::{AgentQueue, QueueState, UnknownSessions, allocate};
use neomax_core::tasks::{TaskRegistry, TaskStatus, TaskStore};

use super::support::{assert_fixture_is_sanitized, fixture_as, fixture_json, fixture_text};

#[test]
fn project_registry_fixture_is_portable_and_preserves_unknown_fields() {
    assert_fixture_is_sanitized("projects/registry.json");
    let expected = fixture_json("projects/registry.json");
    let projects: BTreeMap<String, Project> = serde_json::from_value(expected).unwrap();
    let project = &projects["project-a"];
    assert_eq!(project.root.to_string_lossy(), "/workspace/project-a");
    assert_eq!(project.repos.len(), 2);
    assert_eq!(
        project.agents.as_deref().unwrap().to_string_lossy(),
        "AGENTS.md"
    );
    assert_eq!(project.extra["future_project_field"]["preserve"], true);
}

#[test]
fn task_registry_fixture_filters_done_tasks_and_preserves_flattened_fields() {
    let registry: TaskRegistry = fixture_as("projects/tasks.json");
    assert_eq!(registry.seq, 3);
    assert_eq!(registry.tasks["t1"].status, TaskStatus::Doing);
    assert_eq!(registry.tasks["t2"].status, TaskStatus::Done);
    assert_eq!(registry.tasks["t1"].data["future_task_field"], "preserve");

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tasks.json");
    fs::write(&path, fixture_text("projects/tasks.json")).unwrap();
    let store = TaskStore::new(&path);
    assert_eq!(store.list(Some("project-a"), false).len(), 1);
    assert_eq!(store.list(Some("project-a"), true).len(), 2);

    let malformed = temp.path().join("malformed-tasks.json");
    fs::write(&malformed, "{").unwrap();
    let strict = TaskStore::new(&malformed);
    assert!(strict.try_load().is_err());
    assert_eq!(fs::read_to_string(malformed).unwrap(), "{");
}

#[test]
fn queue_fixture_roundtrips_and_allocation_is_deterministic() {
    let expected = fixture_json("projects/queue.json");
    let mut state: QueueState = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(state.metrics().used, 8);
    assert_eq!(state.metrics().free, 0);
    assert_eq!(state.metrics().active_tasks, 2);
    let serialized = serde_json::to_value(&state).unwrap();
    assert_eq!(serialized["agent_budget"], expected["agent_budget"]);
    assert_eq!(serialized["task_budget"], expected["task_budget"]);
    assert_eq!(serialized["queue"][0]["id"], expected["queue"][0]["id"]);
    assert_eq!(
        serialized["queue"][0]["ts"].as_f64(),
        expected["queue"][0]["ts"].as_f64()
    );
    assert_eq!(
        serialized["queue"][1]["ts"].as_f64(),
        expected["queue"][1]["ts"].as_f64()
    );

    state.queue[1].granted = 0;
    state.agent_budget = 10;
    allocate(&mut state);
    assert_eq!(state.queue[1].granted, 4);
    assert_eq!(state.metrics().free, 1);

    let temp = tempfile::tempdir().unwrap();
    let malformed = temp.path().join("queue.json");
    fs::write(&malformed, "{").unwrap();
    let queue = AgentQueue::new(&malformed, 8, 3, 43_200.0);
    let recovered = queue.snapshot(1_787_488_123.0, &UnknownSessions).unwrap();
    assert_eq!(recovered.agent_budget, 8);
    assert_eq!(recovered.task_budget, 3);
    assert!(recovered.queue.is_empty());
}
