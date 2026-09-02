use neomax_core::orchestration::continuation::ContinuationMode;
use neomax_core::orchestration::handoff::LaunchPlan;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HandoffExecution {
    pub plan: LaunchPlan,
    pub run_id: Option<String>,
    pub continuation: Option<ContinuationMode>,
    pub launched_pid: Option<u32>,
}
