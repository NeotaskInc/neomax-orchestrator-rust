use std::collections::BTreeMap;

use super::*;
use crate::settings::{ConcurrencySettings, EffectiveSettings};

struct Sessions(BTreeMap<String, SessionState>);

impl SessionLiveness for Sessions {
    fn state(&self, session: &str) -> SessionState {
        self.0
            .get(session)
            .copied()
            .unwrap_or(SessionState::Unknown)
    }
}

fn queue(root: &std::path::Path, agents: u32, tasks: u32) -> AgentQueue {
    AgentQueue::new(root.join("agent-queue.json"), agents, tasks, 100.0)
}

#[test]
fn allocates_fifo_and_tops_up_active_work_before_starting_waiters() {
    let temp = tempfile::tempdir().unwrap();
    let queue = queue(temp.path(), 4, 0);
    let sessions = UnknownSessions;
    let first = queue
        .reserve("first", 3, "one", None, 1.0, &sessions)
        .unwrap();
    let second = queue
        .reserve("second", 3, "two", None, 2.0, &sessions)
        .unwrap();
    assert_eq!(first.granted, 3);
    assert_eq!(second.granted, 1);
    let topped = queue
        .reserve("first", 4, "one", None, 3.0, &sessions)
        .unwrap();
    assert_eq!(topped.granted, 3);
    queue
        .release(Some(&second.id), None, 4.0, &sessions)
        .unwrap();
    assert_eq!(
        queue
            .poll(Some(&first.id), None, 5.0, &sessions)
            .unwrap()
            .unwrap()
            .granted,
        4
    );
}

#[test]
fn enforces_the_concurrent_task_cap_without_reordering_fifo() {
    let temp = tempfile::tempdir().unwrap();
    let queue = queue(temp.path(), 10, 1);
    let sessions = UnknownSessions;
    let first = queue
        .reserve("first", 2, "one", None, 1.0, &sessions)
        .unwrap();
    let second = queue
        .reserve("second", 2, "two", None, 2.0, &sessions)
        .unwrap();
    assert_eq!(first.granted, 2);
    assert_eq!(second.granted, 0);
    queue.release(None, Some("first"), 3.0, &sessions).unwrap();
    assert_eq!(
        queue
            .poll(None, Some("second"), 4.0, &sessions)
            .unwrap()
            .unwrap()
            .granted,
        2
    );
}

#[test]
fn reaps_only_expired_or_known_dead_sessions() {
    let temp = tempfile::tempdir().unwrap();
    let queue = queue(temp.path(), 10, 0);
    let unknown = UnknownSessions;
    queue
        .reserve("unknown", 1, "unknown-session", None, 1.0, &unknown)
        .unwrap();
    queue
        .reserve("dead", 1, "dead-session", None, 1.0, &unknown)
        .unwrap();
    queue
        .reserve("pid", 1, "pid-20", None, 1.0, &unknown)
        .unwrap();
    let sessions = Sessions(BTreeMap::from([(
        "dead-session".into(),
        SessionState::Dead,
    )]));
    let state = queue.snapshot(50.0, &sessions).unwrap();
    assert_eq!(
        state
            .queue
            .iter()
            .map(|item| item.task.as_str())
            .collect::<Vec<_>>(),
        ["unknown", "pid"]
    );
    assert!(queue.snapshot(200.0, &sessions).unwrap().queue.is_empty());
}

#[test]
fn reports_budget_metrics_and_persists_runtime_tuning() {
    let temp = tempfile::tempdir().unwrap();
    let queue = queue(temp.path(), 5, 0);
    let sessions = UnknownSessions;
    queue
        .reserve("work", 3, "session", None, 1.0, &sessions)
        .unwrap();
    let state = queue.set_budgets(Some(8), Some(2), 2.0, &sessions).unwrap();
    assert_eq!(
        state.metrics(),
        QueueMetrics {
            agent_budget: 8,
            task_budget: 2,
            used: 3,
            free: 5,
            active_tasks: 1,
            queued_tasks: 1,
        }
    );
}

#[test]
fn reads_the_global_limits_from_effective_settings() {
    let temp = tempfile::tempdir().unwrap();
    let settings = EffectiveSettings {
        concurrency: ConcurrencySettings {
            max_subagents: 73,
            max_tasks: 9,
            ..ConcurrencySettings::default()
        },
        config_path: temp.path().join("config.toml"),
        max_subagents_source: "test".into(),
    };
    let queue = AgentQueue::from_settings(temp.path().join("agent-queue.json"), &settings);
    let state = queue.snapshot(1.0, &UnknownSessions).unwrap();
    assert_eq!(state.agent_budget, 73);
    assert_eq!(state.task_budget, 9);
}

#[test]
fn reads_the_reservation_ttl_from_effective_settings() {
    let temp = tempfile::tempdir().unwrap();
    let settings = EffectiveSettings {
        concurrency: ConcurrencySettings {
            max_subagents: 10,
            max_tasks: 1,
            queue_ttl_seconds: 10.0,
            ..ConcurrencySettings::default()
        },
        config_path: temp.path().join("config.toml"),
        max_subagents_source: "test".into(),
    };
    let queue = AgentQueue::from_settings(temp.path().join("agent-queue.json"), &settings);
    queue
        .reserve("stale", 1, "session", None, 1.0, &UnknownSessions)
        .unwrap();
    assert!(
        queue
            .snapshot(12.0, &UnknownSessions)
            .unwrap()
            .queue
            .is_empty()
    );
}

#[test]
fn malformed_queue_recovers_to_configured_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agent-queue.json");
    std::fs::write(&path, b"{").unwrap();
    let queue = AgentQueue::new(&path, 5, 0, 100.0);
    let state = queue.snapshot(1.0, &UnknownSessions).unwrap();
    assert_eq!(state.agent_budget, 5);
    assert_eq!(state.task_budget, 0);
    assert!(state.queue.is_empty());
    let persisted: QueueState = crate::atomic::read_json(&path).unwrap();
    assert_eq!(persisted, state);
}

#[test]
fn queue_rewrites_preserve_unknown_state_and_reservation_fields() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agent-queue.json");
    let input = serde_json::json!({
        "agent_budget": 4,
        "task_budget": 1,
        "future_state": {"retained": true},
        "queue": [
            {
                "id": "res-valid",
                "task": "valid",
                "want": 1,
                "granted": 1,
                "batch": null,
                "ts": 10.0,
                "session": "pid-42",
                "future_reservation": ["retained", 7]
            },
            null,
            {"id": "bad", "task": 42, "want": 1},
            "not a reservation"
        ]
    });
    std::fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();

    let queue = AgentQueue::new(&path, 7, 3, 100.0);
    let state = queue.snapshot(10.0, &UnknownSessions).unwrap();
    assert_eq!(state.queue.len(), 1);
    assert_eq!(state.extra["future_state"]["retained"], true);
    assert_eq!(
        state.queue[0].extra["future_reservation"],
        serde_json::json!(["retained", 7])
    );

    let persisted: serde_json::Value = crate::atomic::read_json(&path).unwrap();
    assert_eq!(persisted["future_state"]["retained"], true);
    assert_eq!(
        persisted["queue"][0]["future_reservation"],
        serde_json::json!(["retained", 7])
    );
    assert_eq!(persisted["queue"].as_array().unwrap().len(), 1);

    std::fs::write(
        &path,
        br#"{"agent_budget":"invalid","future_state":{"retained":true},"queue":[]}"#,
    )
    .unwrap();
    let recovered = queue.snapshot(10.0, &UnknownSessions).unwrap();
    assert_eq!(recovered.agent_budget, 7);
    assert_eq!(recovered.task_budget, 3);
    assert_eq!(recovered.extra["future_state"]["retained"], true);

    std::fs::write(&path, b"[]").unwrap();
    let non_object = queue.snapshot(10.0, &UnknownSessions).unwrap();
    assert_eq!(non_object.agent_budget, 7);
    assert_eq!(non_object.task_budget, 3);
    assert!(non_object.queue.is_empty());
}

#[test]
fn saturates_u32_queue_aggregation_at_the_budget_ceiling() {
    let mut state = QueueState {
        agent_budget: u32::MAX,
        task_budget: 0,
        queue: vec![
            QueueReservation {
                id: "one".into(),
                task: "one".into(),
                want: u32::MAX,
                granted: u32::MAX,
                batch: None,
                ts: 1.0,
                session: "one".into(),
                extra: Default::default(),
            },
            QueueReservation {
                id: "two".into(),
                task: "two".into(),
                want: u32::MAX,
                granted: u32::MAX,
                batch: None,
                ts: 1.0,
                session: "two".into(),
                extra: Default::default(),
            },
        ],
        extra: Default::default(),
    };

    allocate(&mut state);
    assert_eq!(state.metrics().used, u32::MAX);
    assert_eq!(state.metrics().free, 0);
}
