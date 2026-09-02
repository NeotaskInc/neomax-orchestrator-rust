mod classify;
mod logs;
mod monitor;
mod prepare;
mod process;
mod signals;
pub(crate) mod tooling;
mod types;

#[cfg(all(test, unix))]
mod tests;

pub use classify::{apply_outcome, classify_attempt};
pub use prepare::{PreparedAttempt, prepare_attempt, prepare_attempt_with_secret};
pub use process::AttemptSupervisor;
pub use types::{
    AttemptOutcome, KilledFor, MAX_TIMEOUT_MINUTES, QuotaRotation, SupervisorConfig,
    SupervisorDirective,
};
