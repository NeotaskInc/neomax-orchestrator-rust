use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::runs::RunRecord;

use super::process::{AmbientClaudeProcess, ClaudeProcessSource};
use super::*;

struct Probe {
    alive: BTreeSet<u32>,
}

impl ProcessProbe for Probe {
    fn pid_alive(&self, pid: u32) -> bool {
        self.alive.contains(&pid)
    }

    fn worker_alive(&self, worker_pid: u32, _engine: Engine) -> bool {
        self.alive.contains(&worker_pid)
    }
}

struct Source {
    processes: Vec<AmbientClaudeProcess>,
}

impl ClaudeProcessSource for Source {
    fn processes(&self) -> Result<Vec<AmbientClaudeProcess>> {
        Ok(self.processes.clone())
    }
}

fn run(id: &str, engine: Engine, profile: &str, pid: u32) -> RunRecord {
    let mut run = RunRecord::new(id, engine, "model", "prompt", profile, "/workspace", 1);
    run.status = RunStatus::Running;
    run.worker_pid = Some(pid);
    run
}

#[test]
fn registered_runs_and_ambient_claude_sessions_are_counted_once() {
    let temp = tempfile::tempdir().unwrap();
    let runs = RunStore::new(temp.path().join("runs"));
    let first_profile = temp.path().join("claude-a");
    let second_profile = temp.path().join("claude-b");
    runs.create(&run(
        "registered-claude",
        Engine::Claude,
        &first_profile.to_string_lossy(),
        10,
    ))
    .unwrap();
    runs.create(&run("registered-codex", Engine::Codex, "/codex-a", 20))
        .unwrap();
    let source = Source {
        processes: vec![
            AmbientClaudeProcess {
                pid: 10,
                parent_pid: Some(1),
                profile: first_profile.clone(),
            },
            AmbientClaudeProcess {
                pid: 30,
                parent_pid: Some(1),
                profile: first_profile.clone(),
            },
            AmbientClaudeProcess {
                pid: 30,
                parent_pid: Some(1),
                profile: first_profile.clone(),
            },
            AmbientClaudeProcess {
                pid: 40,
                parent_pid: Some(1),
                profile: second_profile.clone(),
            },
            AmbientClaudeProcess {
                pid: 41,
                parent_pid: Some(40),
                profile: second_profile.clone(),
            },
        ],
    };
    let probe = Probe {
        alive: BTreeSet::from([10, 20]),
    };

    let counts = live_counts_with_source(&runs, &probe, Some(&source)).unwrap();

    assert_eq!(counts.get(&(Engine::Claude, first_profile)), Some(&2));
    assert_eq!(counts.get(&(Engine::Claude, second_profile)), Some(&1));
    assert_eq!(
        counts.get(&(Engine::Codex, PathBuf::from("/codex-a"))),
        Some(&1)
    );
}

#[test]
fn default_constructor_is_hermetic_and_counts_only_registered_runs() {
    let temp = tempfile::tempdir().unwrap();
    let runs = RunStore::new(temp.path().join("runs"));
    let profile = temp.path().join("claude");
    runs.create(&run(
        "registered",
        Engine::Claude,
        &profile.to_string_lossy(),
        10,
    ))
    .unwrap();
    let probe = Probe {
        alive: BTreeSet::from([10]),
    };
    let source = RunLiveWorkSource::new(&runs, &probe);

    let snapshot = source.live_work().unwrap();

    assert_eq!(snapshot.count(Engine::Claude, &profile), 1);
}

#[test]
fn ambient_children_of_registered_workers_are_not_counted_again() {
    let mut counts = std::collections::BTreeMap::new();
    let profile = PathBuf::from("/profiles/claude");
    super::process::add_ambient_counts(
        &mut counts,
        &BTreeSet::from([10]),
        &[
            AmbientClaudeProcess {
                pid: 10,
                parent_pid: Some(1),
                profile: profile.clone(),
            },
            AmbientClaudeProcess {
                pid: 11,
                parent_pid: Some(10),
                profile: profile.clone(),
            },
            AmbientClaudeProcess {
                pid: 12,
                parent_pid: Some(1),
                profile: profile.clone(),
            },
        ],
    );
    assert_eq!(counts.get(&(Engine::Claude, profile)), Some(&1));
}
