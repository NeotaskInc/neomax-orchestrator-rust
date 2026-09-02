use std::collections::BTreeMap;

use crate::Result;
use crate::queue::{SessionLiveness, SessionState};
use crate::runs::{ProbeState, ProcessProbe};

use super::OrchestratorStore;

#[derive(Debug, Clone, Default)]
pub struct OrchestratorLiveness {
    sessions: BTreeMap<String, SessionState>,
}

impl OrchestratorLiveness {
    pub fn load(store: &OrchestratorStore, probe: &impl ProcessProbe, now: i64) -> Result<Self> {
        let sessions = store
            .all(probe, now)?
            .into_iter()
            .map(|record| {
                let state = match record.process_state {
                    ProbeState::Alive => SessionState::Live,
                    ProbeState::Dead => SessionState::Dead,
                    ProbeState::Unknown => SessionState::Unknown,
                };
                (record.session, state)
            })
            .collect();
        Ok(Self { sessions })
    }
}

impl SessionLiveness for OrchestratorLiveness {
    fn state(&self, session: &str) -> SessionState {
        self.sessions
            .get(session)
            .copied()
            .unwrap_or(SessionState::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;
    use crate::orchestration::registry::OrchestratorRegistration;

    struct Probe;

    impl ProcessProbe for Probe {
        fn pid_alive(&self, pid: u32) -> bool {
            pid == 41
        }

        fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
            false
        }
    }

    fn registration(session: &str, pid: u32) -> OrchestratorRegistration {
        OrchestratorRegistration {
            session: session.into(),
            pid: Some(pid),
            engine: Engine::Codex,
            account: Some(1),
            account_dir: ".codex".into(),
            project: None,
            branch_prefix: None,
            cwd: "/workspace".into(),
            model: "gpt-5.6-sol".into(),
            reserved: false,
            now: 10,
        }
    }

    #[test]
    fn distinguishes_live_dead_and_unregistered_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let store = OrchestratorStore::new(temp.path());
        store.register(registration("live", 41)).unwrap();
        store.register(registration("dead", 42)).unwrap();

        let liveness = OrchestratorLiveness::load(&store, &Probe, 10).unwrap();
        assert_eq!(liveness.state("live"), SessionState::Live);
        assert_eq!(liveness.state("dead"), SessionState::Dead);
        assert_eq!(liveness.state("unknown"), SessionState::Unknown);
    }
}
