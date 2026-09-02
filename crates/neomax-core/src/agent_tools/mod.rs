mod commands;
mod environment;
mod guard;
mod invocation;
mod manifest;
mod persistence;
mod policy;
mod prepared;
mod resolution;
mod role;
mod types;

pub const MANIFEST_RELATIVE_PATH: &str = "agent-tools/manifest.json";

pub use commands::CANONICAL_COMMANDS;
pub use environment::{
    EnvironmentInput, NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV, NEOMAX_BIN_ENV, NEOMAX_TOOL_DEPTH_ENV,
    NEOMAX_TOOL_INSTRUCTION_ENV, NEOMAX_TOOL_MANIFEST_ENV, NEOMAX_TOOL_MAX_DEPTH_ENV,
    NEOMAX_TOOL_POLICY_ENV, ToolEnvironment, augment_path, build_environment,
};
pub use guard::{DEFAULT_MAX_DEPTH, MAX_ALLOWED_DEPTH, RecursionGuard};
pub use invocation::resolve_agent_command;
pub use manifest::{
    AgentToolManifest, MANIFEST_SCHEMA_VERSION, ORCHESTRATOR_TOOL_INSTRUCTION, TOOL_INSTRUCTION,
    ToolManifest,
};
pub use persistence::ManifestStore;
pub use policy::{AuthorizedCommand, ToolPolicy};
pub use prepared::PreparedWorkerTools;
pub use resolution::{ExecutableInputs, ExecutableSource, ResolvedExecutable, resolve_executable};
pub use role::LaunchRole;
pub use types::{CanonicalCommand, CommandClass, CommandFamily, ManifestCommand, OrchestratorHost};

#[cfg(test)]
mod tests;
