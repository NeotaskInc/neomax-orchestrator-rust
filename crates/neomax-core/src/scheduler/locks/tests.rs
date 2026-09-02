use std::fs;
use std::path::Path;
use std::sync::{Arc, Barrier};

use super::*;
use crate::Engine;
use crate::runs::{ProbeState, ProcessProbe, RunRecord, RunStatus, RunStore};

#[derive(Clone, Copy)]
struct Probe {
    alive: bool,
}

impl ProcessProbe for Probe {
    fn pid_alive(&self, _pid: u32) -> bool {
        self.alive
    }

    fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
        self.alive
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

fn manager(root: &Path, now: i64) -> AreaLockManager<FallbackTtlLiveness> {
    AreaLockManager::new(root.join("locks"), FallbackTtlLiveness::new(now), now)
}

#[test]
fn fresh_specific_area_acquisition_survives_the_global_probe() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let locks = manager(temp.path(), 100);

    assert!(locks.acquire_area_lock(&repo, "src/core", "run-a"));
    assert!(!locks.acquire_area_lock(&repo, "src/core", "run-b"));
    locks.release_area_locks(&repo, ["src/core"], "run-a");
}

fn run(id: &str, status: RunStatus, supervisor: u32, worker: u32) -> RunRecord {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "engine": "codex",
        "status": status,
        "started": 1,
        "pid": supervisor,
        "worker_pid": worker
    }))
    .unwrap()
}

#[test]
fn maps_locks_to_a_repo_hash_and_keeps_repositories_isolated() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("repo-a");
    let second = temp.path().join("repo-b");
    let locks = manager(temp.path(), 100);
    assert!(locks.acquire_area_lock(&first, "src", "a"));
    assert!(!locks.acquire_area_lock(&first, "src", "b"));
    assert!(locks.acquire_area_lock(&second, "src", "b"));
    assert!(
        locks
            .lock_path(&first, "src")
            .unwrap()
            .starts_with(temp.path().join("locks"))
    );
}

#[test]
fn reentrant_acquisition_and_ownership_checked_release_are_safe() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let locks = manager(temp.path(), 100);
    assert!(locks.acquire_area_lock(&repo, "src", "same"));
    assert!(locks.acquire_area_lock(&repo, "src", "same"));
    locks.release_area_locks(&repo, ["src"], "other");
    assert!(!locks.acquire_area_lock(&repo, "src", "other"));
    locks.release_area_locks(&repo, ["src"], "same");
    assert!(locks.acquire_area_lock(&repo, "src", "other"));
}

#[test]
fn global_area_is_exclusive_but_reentrant_for_the_same_run() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let locks = manager(temp.path(), 100);
    assert!(locks.acquire_area_lock(&repo, "src", "run-a"));
    assert!(!locks.acquire_area_lock(&repo, "*", "run-b"));
    assert!(locks.acquire_area_lock(&repo, "*", "run-a"));
    assert!(!locks.acquire_area_lock(&repo, "docs", "run-b"));
    locks.release_area_locks(&repo, ["src", "*"], "run-a");
    assert!(locks.acquire_area_lock(&repo, "*", "run-b"));
}

#[test]
fn malformed_and_expired_owners_are_reclaimed() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let locks = manager(temp.path(), 100);
    let torn = locks.lock_path(&repo, "torn").unwrap();
    fs::write(&torn, b"{\"runid\":").unwrap();
    assert!(locks.acquire_area_lock(&repo, "torn", "new"));

    let expired = locks.lock_path(&repo, "expired").unwrap();
    let owner = LockOwner {
        runid: "old".into(),
        pid: None,
        ts: 100 - FALLBACK_TTL_SECONDS - 1,
    };
    fs::write(&expired, serde_json::to_vec(&owner).unwrap()).unwrap();
    assert!(locks.acquire_area_lock(&repo, "expired", "new"));
}

#[test]
fn run_store_liveness_reclaims_terminal_or_dead_runs_but_not_live_runs() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let runs = RunStore::new(temp.path().join("runs"));
    runs.create(&run("terminal", RunStatus::Done, 1, 2))
        .unwrap();
    let terminal_probe = Probe { alive: true };
    let terminal_liveness = RunStoreLiveness::new(&runs, &terminal_probe, 100);
    let terminal_locks =
        AreaLockManager::new(temp.path().join("terminal-locks"), terminal_liveness, 100);
    let terminal_path = terminal_locks.lock_path(&repo, "src").unwrap();
    fs::write(
        &terminal_path,
        serde_json::to_vec(&LockOwner {
            runid: "terminal".into(),
            pid: Some(1),
            ts: 100,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(terminal_locks.acquire_area_lock(&repo, "src", "new"));

    runs.create(&run("live", RunStatus::Running, 3, 4)).unwrap();
    let live_probe = Probe { alive: true };
    let live_liveness = RunStoreLiveness::new(&runs, &live_probe, 100);
    let live_locks = AreaLockManager::new(temp.path().join("live-locks"), live_liveness, 100);
    let live_path = live_locks.lock_path(&repo, "src").unwrap();
    fs::write(
        &live_path,
        serde_json::to_vec(&LockOwner {
            runid: "live".into(),
            pid: Some(3),
            ts: 1,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(!live_locks.acquire_area_lock(&repo, "src", "new"));

    let dead_probe = Probe { alive: false };
    let dead_liveness = RunStoreLiveness::new(&runs, &dead_probe, 100);
    let dead_locks = AreaLockManager::new(temp.path().join("dead-locks"), dead_liveness, 100);
    let dead_path = dead_locks.lock_path(&repo, "src").unwrap();
    fs::write(
        &dead_path,
        serde_json::to_vec(&LockOwner {
            runid: "live".into(),
            pid: Some(3),
            ts: 100,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(dead_locks.acquire_area_lock(&repo, "src", "new"));
}

#[test]
fn unknown_process_liveness_keeps_a_lock_busy() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let runs = RunStore::new(temp.path().join("runs"));
    runs.create(&run("uncertain", RunStatus::Running, 3, 4))
        .unwrap();
    let liveness = RunStoreLiveness::new(&runs, &UnknownProbe, 100);
    let locks = AreaLockManager::new(temp.path().join("locks"), liveness, 100);
    let path = locks.lock_path(&repo, "src").unwrap();
    fs::write(
        path,
        serde_json::to_vec(&LockOwner {
            runid: "uncertain".into(),
            pid: Some(3),
            ts: 100,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(!locks.acquire_area_lock(&repo, "src", "new"));
}

#[test]
fn concurrent_managers_allow_one_owner_for_a_shared_area() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let locks = Arc::new(manager(temp.path(), 100));
    let barrier = Arc::new(Barrier::new(8));
    let results = std::thread::scope(|scope| {
        (0..8)
            .map(|index| {
                let locks = Arc::clone(&locks);
                let barrier = Arc::clone(&barrier);
                let repo = repo.clone();
                scope.spawn(move || {
                    barrier.wait();
                    locks.acquire_area_lock(&repo, "packages/core", &format!("run-{index}"))
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(results.into_iter().filter(|value| *value).count(), 1);
}

#[test]
fn multi_area_acquisition_is_all_or_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    let locks = manager(temp.path(), 100);
    assert!(locks.acquire_area_lock(&repo, "docs", "existing"));
    assert!(!locks.acquire_areas(&repo, ["src/core", "docs"], "batch"));
    assert!(locks.acquire_area_lock(&repo, "src/core", "other"));
    locks.release_area_locks(&repo, ["docs"], "existing");
    locks.release_area_locks(&repo, ["src/core"], "other");
    assert!(locks.acquire_areas(&repo, ["docs", "src/core"], "batch"));
}
