use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use neomax_core::orchestration::registry::OrchestratorLiveness;
use neomax_core::runs::{ProcessProbe, RunRecord, RunStatus};
use neomax_core::{EffectiveSettings, Engine, SettingsFile, StatePaths};
use tempfile::TempDir;

use crate::context::RuntimeContext;
use crate::operations::run_lifecycle::process::ProcessTarget;
use crate::operations::run_lifecycle::{ProcessControl, RunExecutor};

pub(crate) struct Fixture {
    pub temp: TempDir,
    pub context: RuntimeContext,
}

pub(crate) fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("fixture temp");
    let home = temp.path().join("home");
    let cwd = temp.path().join("workspace");
    let state = temp.path().join("state");
    std::fs::create_dir_all(&home).expect("fixture home");
    std::fs::create_dir_all(&cwd).expect("fixture cwd");
    let paths = StatePaths::new(home, state);
    paths.ensure_runtime_dirs().expect("fixture dirs");
    let settings = EffectiveSettings::resolve(
        SettingsFile::default(),
        paths.state.join("config.toml"),
        &BTreeMap::new(),
    )
    .expect("fixture settings");
    Fixture {
        context: RuntimeContext::for_test(
            paths,
            settings,
            cwd,
            1_700_000_000,
            OrchestratorLiveness::default(),
            None,
        ),
        temp,
    }
}

pub(crate) fn run(id: &str, status: RunStatus, workdir: PathBuf) -> RunRecord {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "engine": "claude",
        "model": "claude-fable-5[1m]",
        "prompt": "fixture work",
        "profile": "/profiles/.claude1",
        "workdir": workdir,
        "status": status,
        "started": 1,
        "attempt": 1,
        "acknowledged": false
    }))
    .expect("fixture run")
}

#[derive(Clone, Default)]
pub(crate) struct FakeProcess {
    pub supervisors: Arc<Mutex<Vec<u32>>>,
    pub workers: Arc<Mutex<Vec<u32>>>,
    pub terminated: Arc<Mutex<Vec<(u32, ProcessTarget)>>>,
}

impl ProcessProbe for FakeProcess {
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

impl ProcessControl for FakeProcess {
    fn terminate(&self, pid: u32, target: ProcessTarget) -> anyhow::Result<()> {
        self.terminated
            .lock()
            .expect("termination lock")
            .push((pid, target));
        Ok(())
    }
}

pub(crate) struct FakeExecutor {
    pub status: RunStatus,
}

impl RunExecutor for FakeExecutor {
    fn execute(&self, run: &mut RunRecord) -> neomax_core::Result<RunStatus> {
        run.result_text = Some("fixture complete".into());
        Ok(self.status)
    }
}

pub(crate) struct FakeSelector {
    pub profile: PathBuf,
}

impl crate::operations::run_lifecycle::RetryAccountSelector for FakeSelector {
    fn select(
        &self,
        _run: &RunRecord,
        _selector: &super::super::control::ports::RetrySelector,
        _excluded: &std::collections::BTreeSet<PathBuf>,
    ) -> neomax_core::Result<PathBuf> {
        Ok(self.profile.clone())
    }
}
