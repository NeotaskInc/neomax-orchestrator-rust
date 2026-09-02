use std::time::Duration;

use super::claim::IssueClaim;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimOwnerState {
    Live,
    Dead,
    Unknown,
}

pub trait ClaimLiveness: Send + Sync {
    fn session_state(&self, session: &str) -> ClaimOwnerState;
}

pub trait ProcessLiveness: Send + Sync {
    fn pid_alive(&self, pid: u32) -> bool;
}

impl IssueClaim {
    pub fn is_active(
        &self,
        now: i64,
        ttl: Duration,
        liveness: &impl ClaimLiveness,
        processes: &impl ProcessLiveness,
    ) -> bool {
        let age = now.saturating_sub(self.ts);
        if age < 0 || age > ttl.as_secs() as i64 {
            return false;
        }
        match self.session.as_deref() {
            Some(session) if !session.starts_with("pid-") => {
                liveness.session_state(session) == ClaimOwnerState::Live
            }
            _ => self.pid.is_some_and(|pid| processes.pid_alive(pid)),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoLiveClaims;

impl ClaimLiveness for NoLiveClaims {
    fn session_state(&self, _session: &str) -> ClaimOwnerState {
        ClaimOwnerState::Dead
    }
}

impl ProcessLiveness for NoLiveClaims {
    fn pid_alive(&self, _pid: u32) -> bool {
        false
    }
}
