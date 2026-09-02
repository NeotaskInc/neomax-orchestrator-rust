use std::path::PathBuf;

use chrono::{DateTime, Utc};
use neomax_core::Engine;
use neomax_core::accounts::AccountSnapshot;

use crate::context::RuntimeContext;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HandoffSelection {
    pub engine: Engine,
    pub current_profile: PathBuf,
    pub source: AccountSnapshot,
    pub target: Option<neomax_core::orchestration::handoff::TargetSelection>,
    pub check: neomax_core::orchestration::handoff::HandoffCheck,
}

pub(crate) fn context_time(context: &RuntimeContext) -> DateTime<Utc> {
    DateTime::from_timestamp(context.now, 0).unwrap_or_else(Utc::now)
}
