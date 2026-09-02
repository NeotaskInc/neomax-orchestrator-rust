use std::env;

use neomax_core::issues::{ClaimLiveness, ClaimOwnerState, IssueStore, ProcessLiveness};
use neomax_core::orchestration::registry::OrchestratorLiveness;
use neomax_core::queue::{SessionLiveness, SessionState};
use neomax_core::runs::SystemProcessProbe;

use crate::context::RuntimeContext;

pub(super) fn issue_store(context: &RuntimeContext) -> IssueStore {
    IssueStore::with_config(
        context.paths.state.join("issues"),
        neomax_core::issues::IssueStoreConfig {
            events_directory: Some(context.paths.issue_events.clone()),
            ..neomax_core::issues::IssueStoreConfig::default()
        },
    )
}

pub(super) fn current_session() -> Option<String> {
    env::var("NEOMAX_ORCH_SESSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

pub(super) struct RuntimeClaimLiveness<'a>(pub(super) &'a OrchestratorLiveness);

impl ClaimLiveness for RuntimeClaimLiveness<'_> {
    fn session_state(&self, session: &str) -> ClaimOwnerState {
        match self.0.state(session) {
            SessionState::Live => ClaimOwnerState::Live,
            SessionState::Dead => ClaimOwnerState::Dead,
            SessionState::Unknown => ClaimOwnerState::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct IssueProcessProbe;

impl ProcessLiveness for IssueProcessProbe {
    fn pid_alive(&self, pid: u32) -> bool {
        neomax_core::runs::ProcessProbe::pid_alive(&SystemProcessProbe, pid)
    }
}

pub(super) fn active_claim_belongs_elsewhere(
    issue: &neomax_core::issues::Issue,
    context: &RuntimeContext,
) -> bool {
    let Some(claim) = issue.claim.as_ref() else {
        return false;
    };
    if !claim.is_active(
        context.now,
        neomax_core::issues::IssueStoreConfig::default().claim_ttl,
        &RuntimeClaimLiveness(&context.liveness),
        &IssueProcessProbe,
    ) {
        return false;
    }
    let current_session = current_session();
    claim.session != current_session && claim.pid != Some(std::process::id())
}
