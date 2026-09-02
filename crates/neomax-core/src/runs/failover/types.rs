use crate::accounts::AccountSnapshot;

#[derive(Debug, Clone, PartialEq)]
pub struct FailoverTarget {
    pub account: AccountSnapshot,
    pub crosses_provider: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverStop {
    TerminalStatus,
    Disabled,
    ResumedRun,
    AttemptsExhausted,
    NoEligibleAccount,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FailoverDecision {
    Continue(FailoverTarget),
    Stop(FailoverStop),
}
