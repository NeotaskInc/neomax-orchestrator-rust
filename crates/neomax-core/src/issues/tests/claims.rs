use std::time::Duration;

use crate::issues::{ClaimLiveness, ClaimOwnerState, IssueClaim, ProcessLiveness};

struct Probe {
    state: ClaimOwnerState,
    pid: bool,
}

impl ClaimLiveness for Probe {
    fn session_state(&self, _session: &str) -> ClaimOwnerState {
        self.state
    }
}

impl ProcessLiveness for Probe {
    fn pid_alive(&self, _pid: u32) -> bool {
        self.pid
    }
}

#[test]
fn session_claims_require_live_owner_and_ttl() {
    let live = Probe {
        state: ClaimOwnerState::Live,
        pid: false,
    };
    let claim = IssueClaim::new(Some("session".into()), Some(7), 100);
    assert!(claim.is_active(100 + 59, Duration::from_secs(60), &live, &live));
    assert!(!claim.is_active(100 + 61, Duration::from_secs(60), &live, &live));
    let dead = Probe {
        state: ClaimOwnerState::Dead,
        pid: true,
    };
    assert!(!claim.is_active(101, Duration::from_secs(60), &dead, &dead));
}

#[test]
fn pid_claims_use_process_probe() {
    let probe = Probe {
        state: ClaimOwnerState::Unknown,
        pid: true,
    };
    let claim = IssueClaim::new(Some("pid-123".into()), Some(123), 5);
    assert!(claim.is_active(6, Duration::from_secs(10), &probe, &probe));
}
