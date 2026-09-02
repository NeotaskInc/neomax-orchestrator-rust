use neomax_core::runs::RunRecord;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct RotationReport {
    pub(crate) run_id: String,
    pub(crate) status: String,
    pub(crate) source_engine: String,
    pub(crate) source_account: String,
    pub(crate) target_engine: Option<String>,
    pub(crate) target_account: Option<String>,
    pub(crate) attempt: u32,
    pub(crate) crosses_provider: bool,
}

pub(crate) fn without_target(run: &RunRecord, status: String) -> RotationReport {
    RotationReport {
        run_id: run.id.clone(),
        status,
        source_engine: run.engine.to_string(),
        source_account: run.account(),
        target_engine: None,
        target_account: None,
        attempt: run.attempt,
        crosses_provider: false,
    }
}

pub(crate) fn failover_stop_message(stop: neomax_core::runs::failover::FailoverStop) -> String {
    match stop {
        neomax_core::runs::failover::FailoverStop::TerminalStatus => "terminal status".into(),
        neomax_core::runs::failover::FailoverStop::Disabled => "rotation disabled".into(),
        neomax_core::runs::failover::FailoverStop::ResumedRun => "resumed run".into(),
        neomax_core::runs::failover::FailoverStop::AttemptsExhausted => {
            "no eligible account (attempt budget exhausted)".into()
        }
        neomax_core::runs::failover::FailoverStop::NoEligibleAccount => {
            "no eligible account".into()
        }
    }
}
