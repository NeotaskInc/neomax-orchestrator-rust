//! Interactive provider command construction for a Neomax orchestrator.

pub(crate) mod command;
mod types;
mod validation;

pub use command::{build, build_bootstrap};
pub use types::{
    KIMI_AGENT_FILE_RELATIVE_PATH, ORCHESTRATOR_INSTRUCTION_ENV, ORCHESTRATOR_ORIENTATION_ENV,
    OrchestratorEnvironment, OrchestratorRequest, kimi_agent_file,
};

#[cfg(test)]
mod tests;
