mod commands;
mod compat;
mod config_seed;
mod credential_health;
mod discovery;
mod eligibility;
mod entry;
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
mod profile_identity;
mod profile_paths;
mod profiles;
mod ranking;
mod selectors;
mod specs;
mod types;

pub use commands::{
    CommandOutput, CommandRunner, DEFAULT_DISCOVERY_TIMEOUT, DEFAULT_MAX_STDOUT_BYTES,
    DiscoveryCommand, LocalCommandRunner,
};
pub use compat::{
    current_binary, current_profiles, discover_profiles, profile_account_number, provider_profiles,
    worker_profiles,
};
pub use config_seed::codex_config_seed;
pub use credential_health::{CredentialEvidence, CredentialHealth, credential_evidence};
pub use discovery::ProviderDiscovery;
pub use eligibility::{Eligibility, orchestrator_eligibility, worker_eligibility};
pub use entry::{ProfileEntry, resolve_profile_entry, verify_profile_entry_identity};
pub use environment::{Environment, MapEnvironment, ProcessEnvironment};
pub use filesystem::{FileSystem, RealFileSystem};
pub use models::{ModelDefaults, default_models, resolve_model};
pub use profile_identity::{profile_email, profile_email_with_environment};
pub use profiles::{
    checked_claude_keychain_service, claude_keychain_service, codex_auth_identity, credential_path,
    credential_path_with_environment, discover_profile_snapshots, grok_auth_identity,
    inspect_profile_snapshot, resolve_profile_path, worker_profile_snapshots,
};
pub use ranking::{DEFAULT_NEOMAX_PRIORITY, RankingPolicy, choose_neomax, rank_neomax};
pub use selectors::resolve_profile_selector;
pub use specs::{
    CLAUDE_DEFAULT_MODEL, CLAUDE_OPUS_MODEL, CLAUDE_OPUS_MODEL_1M, CODEX_DEFAULT_MODEL,
    CODEX_FAST_ENV, CODEX_SERVICE_TIER, CODEX_SUBAGENT_MODELS, GROK_DEFAULT_MODEL,
    KIMI_DEFAULT_MODEL, OPENCODE_DEFAULT_MODEL, all_specs, codex_service_tier, default_model_id,
    spec, supports_native_interactive_resume, supports_native_resume,
};
pub use types::{
    AuthMethod, AuthStatus, BinaryStatus, CatalogSnapshot, CodexAuthIdentity, GrokAuthIdentity,
    ModelDiscoverySupport, ModelOrigin, OrchestratorCandidate, ProfileEligibility, ProfileSelector,
    ProfileSnapshot, ProviderCapabilities, ProviderSnapshot, ProviderSpec, ResolvedModel,
};

#[cfg(test)]
mod tests;
