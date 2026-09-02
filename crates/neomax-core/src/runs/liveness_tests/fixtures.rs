use crate::Engine;

use super::super::{ProbeState, ProcessProbe, RunRecord};

pub(super) struct Probe {
    pub(super) supervisor: bool,
    pub(super) worker: bool,
}

impl ProcessProbe for Probe {
    fn pid_alive(&self, _pid: u32) -> bool {
        self.supervisor
    }

    fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
        self.worker
    }
}

pub(super) struct UnknownProbe;

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

pub(super) fn run() -> RunRecord {
    serde_json::from_value(serde_json::json!({
        "id":"run", "engine":"codex", "status":"running", "started":1,
        "pid":10, "worker_pid":11, "acknowledged":false
    }))
    .unwrap()
}
