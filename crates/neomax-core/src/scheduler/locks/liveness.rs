use std::io;

use crate::Error;
use crate::runs::{ProbeState, ProcessProbe, RunStore, effective_status, worker_state};

use super::LockOwner;

pub const FALLBACK_TTL_SECONDS: i64 = 6 * 60 * 60;

pub trait LockLiveness: Send + Sync {
    fn is_stale(&self, owner: Option<&LockOwner>) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub struct FallbackTtlLiveness {
    now: i64,
    ttl_seconds: i64,
}

impl FallbackTtlLiveness {
    pub fn new(now: i64) -> Self {
        Self {
            now,
            ttl_seconds: FALLBACK_TTL_SECONDS,
        }
    }

    pub fn with_ttl(now: i64, ttl_seconds: i64) -> Self {
        Self {
            now,
            ttl_seconds: ttl_seconds.max(0),
        }
    }
}

impl LockLiveness for FallbackTtlLiveness {
    fn is_stale(&self, owner: Option<&LockOwner>) -> bool {
        let Some(owner) = owner else {
            return true;
        };
        self.now.saturating_sub(owner.ts) > self.ttl_seconds
    }
}

pub struct RunStoreLiveness<'a, P: ProcessProbe> {
    runs: &'a RunStore,
    probe: &'a P,
    now: i64,
    ttl_seconds: i64,
}

impl<'a, P: ProcessProbe> RunStoreLiveness<'a, P> {
    pub fn new(runs: &'a RunStore, probe: &'a P, now: i64) -> Self {
        Self {
            runs,
            probe,
            now,
            ttl_seconds: FALLBACK_TTL_SECONDS,
        }
    }

    pub fn with_ttl(runs: &'a RunStore, probe: &'a P, now: i64, ttl_seconds: i64) -> Self {
        Self {
            runs,
            probe,
            now,
            ttl_seconds: ttl_seconds.max(0),
        }
    }

    fn fallback_stale(&self, owner: &LockOwner) -> bool {
        if self.now.saturating_sub(owner.ts) > self.ttl_seconds {
            return true;
        }
        owner
            .pid
            .is_some_and(|pid| self.probe.pid_state(pid) == ProbeState::Dead)
    }
}

impl<P: ProcessProbe + Send + Sync> LockLiveness for RunStoreLiveness<'_, P> {
    fn is_stale(&self, owner: Option<&LockOwner>) -> bool {
        let Some(owner) = owner else {
            return true;
        };
        match self.runs.load(&owner.runid) {
            Ok(run) => {
                let status = effective_status(&run, self.probe);
                let supervisor_state = run
                    .supervisor_pid
                    .map_or(ProbeState::Dead, |pid| self.probe.pid_state(pid));
                let worker_liveness = worker_state(&run, self.probe);
                status.is_terminal()
                    || (supervisor_state == ProbeState::Dead && worker_liveness == ProbeState::Dead)
            }
            Err(Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                self.fallback_stale(owner)
            }
            Err(_) => false,
        }
    }
}
