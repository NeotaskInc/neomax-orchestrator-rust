use std::sync::{Arc, Mutex};

use neomax_core::Engine;
use neomax_core::runs::{ProcessProbe, RunRecord, RunStatus, RunStore};

use super::super::process::ProcessTarget;
use super::fixture::{FakeExecutor, FakeProcess, FakeSelector, fixture, run};
use crate::operations::run_lifecycle::{
    ProcessControl, RunExecutor, RunLifecycleCommand, RunLifecycleReport, execute_with_process,
};

#[derive(Clone)]
struct ObservingProcess {
    store: RunStore,
    run_id: String,
    supervisors: Arc<Mutex<Vec<u32>>>,
    workers: Arc<Mutex<Vec<u32>>>,
    terminated: Arc<Mutex<Vec<(u32, ProcessTarget)>>>,
    states_at_signal: Arc<Mutex<Vec<RunRecord>>>,
}

impl ObservingProcess {
    fn new(store: RunStore, run_id: &str, supervisors: &[u32], workers: &[u32]) -> Self {
        Self {
            store,
            run_id: run_id.into(),
            supervisors: Arc::new(Mutex::new(supervisors.to_vec())),
            workers: Arc::new(Mutex::new(workers.to_vec())),
            terminated: Arc::new(Mutex::new(Vec::new())),
            states_at_signal: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl ProcessProbe for ObservingProcess {
    fn pid_alive(&self, pid: u32) -> bool {
        self.supervisors
            .lock()
            .expect("supervisor lock")
            .contains(&pid)
    }

    fn worker_alive(&self, worker_pid: u32, _engine: Engine) -> bool {
        self.workers
            .lock()
            .expect("worker lock")
            .contains(&worker_pid)
    }
}

impl ProcessControl for ObservingProcess {
    fn terminate(&self, pid: u32, target: ProcessTarget) -> anyhow::Result<()> {
        let state = self.store.load(&self.run_id).expect("state before signal");
        self.states_at_signal
            .lock()
            .expect("state lock")
            .push(state);
        self.terminated
            .lock()
            .expect("termination lock")
            .push((pid, target));
        match target {
            ProcessTarget::Supervisor => self
                .supervisors
                .lock()
                .expect("supervisor lock")
                .retain(|value| *value != pid),
            ProcessTarget::Worker => self
                .workers
                .lock()
                .expect("worker lock")
                .retain(|value| *value != pid),
        }
        Ok(())
    }
}

#[test]
fn kill_marks_before_signaling_and_preserves_the_worktree_for_resume() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let mut active = run("kill-me", RunStatus::Running, fixture.context.cwd.clone());
    active.worker_pid = Some(43);
    active.worktree = Some(fixture.context.cwd.clone());
    store.create(&active).unwrap();
    let process = FakeProcess {
        workers: std::sync::Arc::new(std::sync::Mutex::new(vec![43])),
        ..FakeProcess::default()
    };
    let report = execute_with_process(
        RunLifecycleCommand::Kill,
        &fixture.context,
        &["kill-me".into()],
        &process,
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::Kill(report) = report else {
        panic!("expected kill report")
    };
    assert!(report.marked);
    assert!(report.terminated);
    assert!(report.worktree_preserved);
    assert!(!report.acknowledged);
    assert_eq!(store.load("kill-me").unwrap().status, RunStatus::Aborted);
    assert_eq!(
        process.terminated.lock().unwrap().as_slice(),
        &[(43, ProcessTarget::Worker)]
    );
}

#[test]
fn kill_persists_aborted_state_and_pid_until_process_control_runs() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let mut active = run(
        "durable-kill",
        RunStatus::Running,
        fixture.context.cwd.clone(),
    );
    active.worker_pid = Some(43);
    active.supervisor_pid = Some(44);
    store.create(&active).unwrap();
    let process = ObservingProcess::new(store.clone(), "durable-kill", &[44], &[43]);

    execute_with_process(
        RunLifecycleCommand::Kill,
        &fixture.context,
        &["durable-kill".into()],
        &process,
        None,
        None,
    )
    .unwrap();

    let states = process.states_at_signal.lock().unwrap();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].status, RunStatus::Aborted);
    assert_eq!(states[0].ended, Some(fixture.context.now));
    assert_eq!(states[0].acknowledged, Some(false));
    assert!(states[0].killed);
    assert_eq!(states[0].worker_pid, Some(43));
    assert_eq!(states[0].supervisor_pid, Some(44));
}

#[test]
fn kill_targets_a_live_worker_before_a_live_supervisor() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let mut active = run(
        "worker-first",
        RunStatus::Running,
        fixture.context.cwd.clone(),
    );
    active.worker_pid = Some(43);
    active.supervisor_pid = Some(44);
    store.create(&active).unwrap();
    let process = ObservingProcess::new(store.clone(), "worker-first", &[44], &[43]);

    let report = execute_with_process(
        RunLifecycleCommand::Kill,
        &fixture.context,
        &["worker-first".into()],
        &process,
        None,
        None,
    )
    .unwrap();

    let RunLifecycleReport::Kill(report) = report else {
        panic!("expected kill report")
    };
    assert_eq!(report.target.as_deref(), Some("worker"));
    assert_eq!(
        process.terminated.lock().unwrap().as_slice(),
        &[(43, ProcessTarget::Worker)]
    );
    let saved = store.load("worker-first").unwrap();
    assert_eq!(saved.worker_pid, None);
    assert_eq!(saved.supervisor_pid, Some(44));
}

#[test]
fn kill_falls_back_to_the_supervisor_when_no_worker_is_live() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let mut active = run(
        "supervisor-fallback",
        RunStatus::Running,
        fixture.context.cwd.clone(),
    );
    active.worker_pid = Some(43);
    active.supervisor_pid = Some(44);
    store.create(&active).unwrap();
    let process = ObservingProcess::new(store.clone(), "supervisor-fallback", &[44], &[]);

    let report = execute_with_process(
        RunLifecycleCommand::Kill,
        &fixture.context,
        &["supervisor-fallback".into()],
        &process,
        None,
        None,
    )
    .unwrap();

    let RunLifecycleReport::Kill(report) = report else {
        panic!("expected kill report")
    };
    assert_eq!(report.target.as_deref(), Some("supervisor"));
    assert_eq!(
        process.terminated.lock().unwrap().as_slice(),
        &[(44, ProcessTarget::Supervisor)]
    );
    let saved = store.load("supervisor-fallback").unwrap();
    assert_eq!(saved.worker_pid, Some(43));
    assert_eq!(saved.supervisor_pid, None);
}

#[test]
fn repeated_kill_is_idempotent_after_the_target_is_terminated() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let mut active = run(
        "repeat-kill",
        RunStatus::Running,
        fixture.context.cwd.clone(),
    );
    active.worker_pid = Some(43);
    store.create(&active).unwrap();
    let process = ObservingProcess::new(store.clone(), "repeat-kill", &[], &[43]);

    execute_with_process(
        RunLifecycleCommand::Kill,
        &fixture.context,
        &["repeat-kill".into()],
        &process,
        None,
        None,
    )
    .unwrap();
    let second = execute_with_process(
        RunLifecycleCommand::Kill,
        &fixture.context,
        &["repeat-kill".into()],
        &process,
        None,
        None,
    )
    .unwrap();

    let RunLifecycleReport::Kill(second) = second else {
        panic!("expected kill report")
    };
    assert!(second.terminated);
    assert!(second.target.is_none());
    assert_eq!(
        process.terminated.lock().unwrap().as_slice(),
        &[(43, ProcessTarget::Worker)]
    );
    let saved = store.load("repeat-kill").unwrap();
    assert_eq!(saved.status, RunStatus::Aborted);
    assert!(saved.killed);
    assert_eq!(saved.worker_pid, None);
}

#[test]
fn kill_leaves_an_already_terminal_run_unchanged() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let terminal = run("already-done", RunStatus::Done, fixture.context.cwd.clone());
    store.create(&terminal).unwrap();
    let process = FakeProcess::default();
    let report = execute_with_process(
        RunLifecycleCommand::Kill,
        &fixture.context,
        &["already-done".into()],
        &process,
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::Kill(report) = report else {
        panic!("expected kill report")
    };
    assert!(!report.marked);
    assert!(report.terminated);
    assert!(!store.load("already-done").unwrap().killed);
}

#[test]
fn retry_selects_a_different_profile_and_executor_finishes_the_attempt() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let run = run("retry-me", RunStatus::Error, fixture.context.cwd.clone());
    store.create(&run).unwrap();
    let target = fixture.temp.path().join("profile-2");
    let selector = FakeSelector {
        profile: target.clone(),
    };
    let executor = FakeExecutor {
        status: RunStatus::Done,
    };
    let process = FakeProcess::default();
    let report = execute_with_process(
        RunLifecycleCommand::Retry,
        &fixture.context,
        &[
            "retry-me".into(),
            "auto".into(),
            "continue carefully".into(),
        ],
        &process,
        Some(&executor),
        Some(&selector),
    )
    .unwrap();
    let RunLifecycleReport::Rerun(report) = report else {
        panic!("expected rerun report")
    };
    assert_eq!(report.status, "done");
    let saved = store.load("retry-me").unwrap();
    assert_eq!(saved.profile, target);
    assert_eq!(saved.attempt, 2);
    assert_eq!(saved.status, RunStatus::Done);
    assert_eq!(saved.prompt_to_send.as_deref(), Some("continue carefully"));
}

#[test]
fn resume_keeps_the_recorded_session_and_rejects_live_work() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let mut resume_record = run("resume-me", RunStatus::Aborted, fixture.context.cwd.clone());
    resume_record.session = Some("session-1".into());
    store.create(&resume_record).unwrap();
    let executor = FakeExecutor {
        status: RunStatus::Done,
    };
    let process = FakeProcess::default();
    execute_with_process(
        RunLifecycleCommand::Resume,
        &fixture.context,
        &["resume-me".into()],
        &process,
        Some(&executor),
        None,
    )
    .unwrap();
    let saved = store.load("resume-me").unwrap();
    assert_eq!(saved.status, RunStatus::Done);
    assert_eq!(saved.session, Some("session-1".into()));

    let mut live = run("live", RunStatus::Running, fixture.context.cwd.clone());
    live.supervisor_pid = Some(44);
    store.create(&live).unwrap();
    let process = FakeProcess {
        supervisors: std::sync::Arc::new(std::sync::Mutex::new(vec![44])),
        ..FakeProcess::default()
    };
    assert!(
        execute_with_process(
            RunLifecycleCommand::Resume,
            &fixture.context,
            &["live".into()],
            &process,
            Some(&executor),
            None,
        )
        .is_err()
    );
}

#[test]
fn codex_resume_starts_a_fresh_thread_and_keeps_failover_enabled() {
    let fixture = fixture();
    let store = RunStore::new(&fixture.context.paths.runs);
    let mut resume_record = run(
        "codex-resume",
        RunStatus::Aborted,
        fixture.context.cwd.clone(),
    );
    resume_record.engine = neomax_core::Engine::Codex;
    resume_record.profile = fixture.temp.path().join(".codex-acct1");
    resume_record.session = Some("old-thread".into());
    store.create(&resume_record).unwrap();

    struct FreshCodexThread;

    impl RunExecutor for FreshCodexThread {
        fn execute(&self, run: &mut RunRecord) -> neomax_core::Result<RunStatus> {
            assert!(!run.resumed);
            assert!(run.resume_session.is_none());
            assert!(run.session.is_none());
            run.session = Some("new-thread".into());
            Ok(RunStatus::Limit)
        }
    }

    let process = FakeProcess::default();
    execute_with_process(
        RunLifecycleCommand::Resume,
        &fixture.context,
        &["codex-resume".into()],
        &process,
        Some(&FreshCodexThread),
        None,
    )
    .unwrap();

    let saved = store.load("codex-resume").unwrap();
    assert!(!saved.resumed);
    assert_eq!(saved.session.as_deref(), Some("new-thread"));
    assert!(saved.resume_session.is_none());
    assert_eq!(saved.session_history[0].session, "old-thread");
}
