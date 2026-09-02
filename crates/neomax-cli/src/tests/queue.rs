use neomax_core::queue::AgentQueue;

use crate::queue;
use crate::tests::fixture;

#[test]
fn queue_uses_effective_max_subagents_and_allocates_waiting_work() {
    let fixture = fixture();
    let mut settings = fixture.context.settings.clone();
    settings.concurrency.max_subagents = 3;
    let context = crate::context::RuntimeContext::for_test(
        fixture.context.paths.clone(),
        settings,
        fixture.context.cwd.clone(),
        fixture.context.now,
        fixture.context.liveness.clone(),
        None,
    );
    queue::run(
        &context,
        &["reserve".into(), "--task=first".into(), "--agents=2".into()],
    )
    .expect("first reservation");
    queue::run(
        &context,
        &[
            "reserve".into(),
            "--task".into(),
            "second".into(),
            "--agents".into(),
            "2".into(),
        ],
    )
    .expect("second reservation");

    let queue = AgentQueue::from_settings(&context.paths.agent_queue, &context.settings);
    let state = queue
        .snapshot(context.now as f64, &context.liveness)
        .expect("queue state");
    assert_eq!(state.agent_budget, 3);
    assert_eq!(state.metrics().used, 3);
    assert_eq!(state.queue.len(), 2);
    assert_eq!(state.queue[0].granted, 2);
    assert_eq!(state.queue[1].granted, 1);
}

#[test]
fn poll_release_and_budget_commands_update_the_shared_queue() {
    let fixture = fixture();
    queue::run(
        &fixture.context,
        &[
            "reserve".into(),
            "--task".into(),
            "task".into(),
            "--agents".into(),
            "2".into(),
        ],
    )
    .expect("reservation");
    let queue = AgentQueue::from_settings(
        &fixture.context.paths.agent_queue,
        &fixture.context.settings,
    );
    let id = queue
        .snapshot(fixture.context.now as f64, &fixture.context.liveness)
        .expect("queue state")
        .queue
        .first()
        .expect("reservation")
        .id
        .clone();
    queue::run(
        &fixture.context,
        &["poll".into(), "--id".into(), id.clone(), "--json".into()],
    )
    .expect("poll reservation");
    queue::run(
        &fixture.context,
        &[
            "set-budget".into(),
            "--agents".into(),
            "7".into(),
            "--tasks=4".into(),
        ],
    )
    .expect("set budget");
    let state = queue
        .snapshot(fixture.context.now as f64, &fixture.context.liveness)
        .expect("updated queue state");
    assert_eq!(state.agent_budget, 7);
    assert_eq!(state.task_budget, 4);
    queue::run(&fixture.context, &["release".into(), "--id".into(), id])
        .expect("release reservation");
    assert!(
        queue
            .snapshot(fixture.context.now as f64, &fixture.context.liveness)
            .expect("empty queue")
            .queue
            .is_empty()
    );
}

#[test]
fn queue_rejects_zero_agent_reservations() {
    let fixture = fixture();
    let error = queue::run(
        &fixture.context,
        &[
            "reserve".into(),
            "--task".into(),
            "task".into(),
            "--agents".into(),
            "0".into(),
        ],
    )
    .expect_err("zero agents should fail");
    assert!(error.to_string().contains("positive integer"));
}
