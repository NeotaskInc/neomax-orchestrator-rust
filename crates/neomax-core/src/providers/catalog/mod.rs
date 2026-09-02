mod commands;
mod compat;
mod discovery;
mod eligibility;
mod environment;
mod filesystem;
mod models;
mod profile_auth;
mod profile_auth_claude;
mod profile_auth_codex;
mod profile_auth_common;
mod profile_auth_grok;
mod profile_auth_kimi;
mod profile_auth_opencode;
mod profile_auth_store;
mod profile_paths;
mod profiles;
mod ranking;
mod specs;
mod types;

pub use commands::{
    CommandOutput, CommandRunner, DiscoveryCommand, LocalCommandRunner, DEFAULT_DISCOVERY_TIMEOUT,
    DEFAULT_MAX_STDOUT_BYTES,
};
pub use compat::{
    current_binary, current_profiles, discover_profiles, profile_account_number, provider_profiles,
    worker_profiles,
};
pub use discovery::ProviderDiscovery;
pub use eligibility::{orchestrator_eligibility, worker_eligibility, Eligibility};
pub use environment::{Environment, MapEnvironment, ProcessEnvironment};
pub use filesystem::{FileSystem, RealFileSystem};
pub use models::{default_models, resolve_model, ModelDefaults};
pub use profiles::{
    checked_claude_keychain_service, claude_keychain_service, codex_auth_identity, credential_path,
    credential_path_with_environment, discover_profile_snapshots, grok_auth_identity,
    inspect_profile_snapshot, resolve_profile_path, worker_profile_snapshots,
};
pub use ranking::{choose_neomax, rank_neomax, RankingPolicy, DEFAULT_NEOMAX_PRIORITY};
pub use specs::{
    all_specs, default_model_id, spec, supports_native_interactive_resume, supports_native_resume,
    CLAUDE_DEFAULT_MODEL, CLAUDE_OPUS_MODEL, CLAUDE_OPUS_MODEL_1M, CODEX_DEFAULT_MODEL,
    CODEX_SERVICE_TIER, GROK_DEFAULT_MODEL, KIMI_DEFAULT_MODEL, OPENCODE_DEFAULT_MODEL,
};
pub use types::{
    AuthMethod, AuthStatus, BinaryStatus, CatalogSnapshot, CodexAuthIdentity, GrokAuthIdentity,
    ModelDiscoverySupport, ModelOrigin, OrchestratorCandidate, ProfileEligibility, ProfileSelector,
    ProfileSnapshot, ProviderCapabilities, ProviderSnapshot, ProviderSpec, ResolvedModel,
};

#[cfg(test)]
mod tests;
