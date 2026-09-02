use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileSelector {
    Number(u32),
    Orchestrator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    OAuth,
    ApiKey,
    Device,
    LocalCredential,
}

/// A non-secret, local identity marker extracted from Codex credentials.
///
/// The marker is deliberately an opaque digest. It is useful for detecting
/// profiles that share one account or refresh-token family without retaining
/// or displaying a token, email address, or provider identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexAuthIdentity {
    label: String,
    plan: Option<String>,
}

impl CodexAuthIdentity {
    pub(crate) fn new(label: String, plan: Option<String>) -> Self {
        Self { label, plan }
    }

    /// Returns a stable, sanitized label suitable for local status output.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the allowlisted plan label from the local JWT, when present.
    pub fn plan(&self) -> Option<&str> {
        self.plan.as_deref()
    }
}

/// Non-secret identity metadata from a local Grok credential entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrokAuthIdentity {
    method: String,
    email: Option<String>,
    name: Option<String>,
    team: Option<String>,
}

impl GrokAuthIdentity {
    pub(crate) fn new(
        method: String,
        email: Option<String>,
        name: Option<String>,
        team: Option<String>,
    ) -> Self {
        Self {
            method,
            email,
            name,
            team,
        }
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn team(&self) -> Option<&str> {
        self.team.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelDiscoverySupport {
    /// The local command is advisory and may omit models supported by the provider.
    BestEffort,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub orchestrator: bool,
    pub worker: bool,
    pub multiple_profiles: bool,
    pub model_discovery: ModelDiscoverySupport,
    pub native_sessions: bool,
    pub usage_discovery: bool,
    pub auth_methods: Vec<AuthMethod>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelOrigin {
    Explicit,
    ProviderEnvironment,
    GlobalEnvironment,
    StrictDefault,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedModel {
    pub id: String,
    pub origin: ModelOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStatus {
    Authenticated { methods: Vec<AuthMethod> },
    Unauthenticated,
    Unknown,
}

impl AuthStatus {
    pub fn is_authenticated(&self) -> bool {
        matches!(self, Self::Authenticated { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileEligibility {
    pub credential_present: bool,
    pub authenticated: bool,
    pub worker_eligible: bool,
    pub orchestrator_eligible: bool,
    /// The profile can participate in an in-place credential copy or swap.
    pub rotation_eligible: bool,
    /// The profile is authenticated and may participate in account pooling.
    /// This does not imply that its credentials may be copied or swapped.
    pub managed_pool_eligible: bool,
}

impl ProfileEligibility {
    pub const fn disconnected() -> Self {
        Self {
            credential_present: false,
            authenticated: false,
            worker_eligible: false,
            orchestrator_eligible: false,
            rotation_eligible: false,
            managed_pool_eligible: false,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileSnapshot {
    pub engine: Engine,
    pub account: String,
    pub path: PathBuf,
    pub reserved: bool,
    pub auth: AuthStatus,
    pub eligibility: ProfileEligibility,
}

impl fmt::Debug for ProfileSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileSnapshot")
            .field("engine", &self.engine)
            .field("account", &self.account)
            .field("path", &self.path)
            .field("reserved", &self.reserved)
            .field("auth", &self.auth)
            .field("eligibility", &self.eligibility)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryStatus {
    pub program: String,
    pub available: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSpec {
    pub engine: Engine,
    pub default_binary: String,
    pub binary_env: String,
    pub config_env: String,
    pub profile_env: String,
    pub default_profile_dir: String,
    pub account_prefix: String,
    pub orchestrator_dir: String,
    pub orchestrator_env: String,
    pub model_env: String,
    pub default_model: String,
    pub model_args: Vec<String>,
    pub default_unsets_config_env: bool,
    pub scrub: Vec<String>,
    pub capabilities: ProviderCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub spec: ProviderSpec,
    pub binary: BinaryStatus,
    pub profiles: Vec<ProfileSnapshot>,
    pub models: Vec<String>,
}

impl ProviderSnapshot {
    pub fn connected(&self) -> bool {
        self.profiles
            .iter()
            .any(|profile| profile.eligibility.authenticated)
    }

    pub fn eligible_for_orchestrator(&self) -> bool {
        self.binary.available
            && self.spec.capabilities.orchestrator
            && self
                .profiles
                .iter()
                .any(|profile| profile.eligibility.orchestrator_eligible)
    }

    pub fn eligible_for_workers(&self) -> bool {
        self.binary.available
            && self.spec.capabilities.worker
            && self
                .profiles
                .iter()
                .any(|profile| profile.eligibility.worker_eligible)
    }

    pub fn authenticated_profiles(&self) -> impl Iterator<Item = &ProfileSnapshot> {
        self.profiles
            .iter()
            .filter(|profile| profile.eligibility.authenticated)
    }

    pub fn managed_profiles(&self) -> impl Iterator<Item = &ProfileSnapshot> {
        self.profiles
            .iter()
            .filter(|profile| profile.eligibility.managed_pool_eligible)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    pub providers: BTreeMap<Engine, ProviderSnapshot>,
}

impl CatalogSnapshot {
    pub fn connected_engines(&self) -> impl Iterator<Item = Engine> + '_ {
        self.providers
            .values()
            .filter(|provider| provider.connected())
            .map(|provider| provider.spec.engine)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrchestratorCandidate {
    pub engine: Engine,
    pub profile: PathBuf,
    pub account: String,
    pub pressure: Option<f64>,
    pub live_workers: u32,
    pub previous: bool,
}

impl OrchestratorCandidate {
    pub fn new(
        profile: &ProfileSnapshot,
        pressure: Option<f64>,
        live_workers: u32,
        previous: bool,
    ) -> Self {
        Self {
            engine: profile.engine,
            profile: profile.path.clone(),
            account: profile.account.clone(),
            pressure,
            live_workers,
            previous,
        }
    }
}
