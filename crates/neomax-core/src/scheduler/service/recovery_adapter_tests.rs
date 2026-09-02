use std::collections::BTreeMap;

use super::super::runtime::{DispatchRequest, WorkerOutcome};
use super::adapters::{CoordinatorRecovery, MAX_RECOVERY_RUN_BYTES};
use super::ports::{RecoveryPort, RecoveryStatus};
use crate::runs::{ProbeState, ProcessProbe, RunRecord, RunStatus, RunStore};
use crate::{Engine, Result};

struct Probe {
    supervisor: bool,
    worker: bool,
}

impl ProcessProbe for Probe {
    fn pid_alive(&self, _pid: u32) -> bool {
        self.supervisor
    }

    fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
        self.worker
    }
}

struct UnknownProbe;

impl ProcessProbe for UnknownProbe {
    fn pid_alive(&self, _pid: u32) -> bool {
        false
    }

    fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
        false
    }

    fn pid_state(&self, _pid: u32) -> ProbeState {
        ProbeState::Unknown
    }

    fn worker_state(&self, _worker_pid: u32, _engine: Engine) -> ProbeState {
        ProbeState::Unknown
    }
}

fn request(run_id: &str) -> DispatchRequest {
    DispatchRequest {
        plan_id: "plan".into(),
        part_id: "part".into(),
        run_id: run_id.into(),
        attempt: 1,
        engine: Engine::Claude,
        model: None,
        prompt: "work".into(),
        areas: Vec::new(),
        dependencies: Vec::new(),
        cwd: "/workspace".into(),
        repository: None,
        branch: None,
        base: None,
        environment: BTreeMap::new(),
    }
}

#[test]
fn recovery_maps_the_durable_provider_record_to_the_scheduler_run_id() -> Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let runs = RunStore::new(temp.path());
    let mut run = RunRecord::new(
        "provider-internal",
        Engine::Claude,
        "fixture",
        "work",
        "/profile",
        "/workspace",
        10,
    );
    run.status = RunStatus::Done;
    run.extra
        .insert("scheduler_run_id".into(), "plan-part".into());
    runs.create(&run)?;
    let mut recovery = CoordinatorRecovery::new(
        temp.path(),
        Probe {
            supervisor: false,
            worker: false,
        },
    );
    let execution = crate::scheduler::PartExecution {
        run_id: Some("plan-part".into()),
        ..Default::default()
    };
    let status = recovery.inspect(&request("plan-part"), &execution)?;
    assert_eq!(
        status,
        RecoveryStatus::Completed(WorkerOutcome::Completed {
            run_id: "plan-part".into()
        })
    );
    Ok(())
}

#[test]
fn recovery_keeps_a_live_supervised_provider_run_running() -> Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let runs = RunStore::new(temp.path());
    let mut run = RunRecord::new(
        "provider-internal",
        Engine::Claude,
        "fixture",
        "work",
        "/profile",
        "/workspace",
        10,
    );
    run.extra
        .insert("scheduler_run_id".into(), "plan-part".into());
    run.supervisor_pid = Some(10);
    runs.create(&run)?;
    let mut recovery = CoordinatorRecovery::new(
        temp.path(),
        Probe {
            supervisor: true,
            worker: false,
        },
    );
    let execution = crate::scheduler::PartExecution {
        run_id: Some("plan-part".into()),
        ..Default::default()
    };
    assert_eq!(
        recovery.inspect(&request("plan-part"), &execution)?,
        RecoveryStatus::StillRunning
    );
    let handle = recovery
        .live_handle(&request("plan-part"), &execution)?
        .expect("live recovery must provide a polling handle");
    let mut handle = handle;
    assert_eq!(handle.poll()?, None);
    runs.update("provider-internal", |record| {
        record.status = RunStatus::Done;
        Ok(())
    })?;
    assert_eq!(
        handle.poll()?,
        Some(crate::scheduler::runtime::WorkerOutcome::Completed {
            run_id: "plan-part".into()
        })
    );
    Ok(())
}

#[test]
fn recovery_fails_closed_on_a_corrupt_matching_provider_record() -> Result<()> {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("plan-part-attempt-1-corrupt.json"),
        b"{\"id\":",
    )?;
    let mut recovery = CoordinatorRecovery::new(
        temp.path(),
        Probe {
            supervisor: false,
            worker: false,
        },
    );
    let execution = crate::scheduler::PartExecution {
        run_id: Some("plan-part".into()),
        ..Default::default()
    };
    let error = recovery
        .inspect(&request("plan-part"), &execution)
        .unwrap_err();
    assert!(error.to_string().contains("corrupt provider run record"));
    Ok(())
}

#[test]
fn recovery_rejects_an_oversized_matching_provider_record() -> Result<()> {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("plan-part-attempt-1-oversized.json"),
        vec![b'x'; MAX_RECOVERY_RUN_BYTES + 1],
    )?;
    let mut recovery = CoordinatorRecovery::new(
        temp.path(),
        Probe {
            supervisor: false,
            worker: false,
        },
    );
    let execution = crate::scheduler::PartExecution {
        run_id: Some("plan-part".into()),
        ..Default::default()
    };
    let error = recovery
        .inspect(&request("plan-part"), &execution)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains(&MAX_RECOVERY_RUN_BYTES.to_string()));
    Ok(())
}

#[test]
fn recovery_fails_closed_when_process_liveness_is_unknown() -> Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let runs = RunStore::new(temp.path());
    let mut run = RunRecord::new(
        "provider-internal",
        Engine::Claude,
        "fixture",
        "work",
        "/profile",
        "/workspace",
        10,
    );
    run.extra
        .insert("scheduler_run_id".into(), "plan-part".into());
    run.supervisor_pid = Some(10);
    runs.create(&run)?;
    let mut recovery = CoordinatorRecovery::new(temp.path(), UnknownProbe);
    let execution = crate::scheduler::PartExecution {
        run_id: Some("plan-part".into()),
        ..Default::default()
    };
    let error = recovery
        .inspect(&request("plan-part"), &execution)
        .expect_err("unknown liveness must not finalize or retry a run");
    assert!(error.to_string().contains("liveness is indeterminate"));
    assert_eq!(runs.load("provider-internal")?.status, RunStatus::Running);
    Ok(())
}

#[test]
fn recovered_worker_cancellation_persists_abort_and_is_idempotent() -> Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let runs = RunStore::new(temp.path());
    let mut run = RunRecord::new(
        "provider-internal",
        Engine::Claude,
        "fixture",
        "work",
        "/profile",
        "/workspace",
        10,
    );
    run.extra
        .insert("scheduler_run_id".into(), "plan-part".into());
    runs.create(&run)?;

    let mut recovery = CoordinatorRecovery::new(
        temp.path(),
        Probe {
            supervisor: false,
            worker: false,
        },
    );
    let execution = crate::scheduler::PartExecution {
        run_id: Some("plan-part".into()),
        ..Default::default()
    };
    let mut handle = recovery
        .live_handle(&request("plan-part"), &execution)?
        .expect("running record must have a cancellation handle");
    handle.cancel()?;
    handle.cancel()?;

    let saved = runs.load("provider-internal")?;
    assert_eq!(saved.status, RunStatus::Aborted);
    assert!(saved.killed);
    assert_eq!(saved.interrupt_signal, Some(15));
    assert_eq!(saved.acknowledged, Some(false));
    assert!(saved.ended.is_some());
    Ok(())
}
