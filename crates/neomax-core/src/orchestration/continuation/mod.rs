mod ports;
mod request;
mod service;
mod state;

pub use ports::{CredentialRotationPort, HandoffPort};
pub use request::{ContinuationRequest, RotationTrigger};
pub use service::{
    ContinuationMode, ContinuationOutcome, ContinuationPort, ContinuationService,
    FilesystemContinuation,
};
pub use state::{apply_to_run, apply_to_run_with_resolver, build_baton};

#[cfg(test)]
mod tests;
