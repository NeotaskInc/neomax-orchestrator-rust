pub mod auth;
pub mod catalog;
mod claude;
mod codex;
mod event_types;
pub mod events;
mod grok;
mod kimi;
pub mod kimi_plan;
mod opencode;
pub mod opencode_policy;
pub mod orchestrator;
pub(crate) mod process_secret;
pub mod runtime;
mod worker;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Engine, Result};

pub use claude::Claude;
pub use codex::Codex;
pub use event_types::{ChildActivity, ParsedEvents, TokenUsage};
pub use events::{
    CODEX_RATE_LIMIT_REFRESH_METHOD, CODEX_RATE_LIMIT_REFRESH_TIMEOUT_MS, CodexQuotaRefreshReason,
    CodexQuotaRefreshRequest, CodexQuotaRefreshResult, apply_codex_quota_refresh,
    apply_refresh_result, codex_quota_refresh_request, refresh_from_rollout, refresh_request,
};
pub use grok::Grok;
pub use kimi::Kimi;
pub use opencode::OpenCode;
pub use orchestrator::{
    KIMI_AGENT_FILE_RELATIVE_PATH, ORCHESTRATOR_INSTRUCTION_ENV, ORCHESTRATOR_ORIENTATION_ENV,
    OrchestratorEnvironment, OrchestratorRequest, build as build_orchestrator_command,
    build_bootstrap as build_orchestrator_bootstrap_command, kimi_agent_file,
};
pub use process_secret::{
    ProviderProcessSecret, is_secret_environment_key, scrub_provider_environment,
    scrub_provider_process_request,
};
pub use runtime::ProviderRuntime;
pub use worker::{
    DIRECTIVE as WORKER_DIRECTIVE, ORCHESTRATOR_DIRECTIVE, ProviderCommand, WorkerLaunchContext,
    WorkerRequest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProfile {
    pub engine: Engine,
    pub account: String,
    pub path: PathBuf,
    pub reserved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    Authenticated,
    Unauthenticated,
    Unknown,
}

pub trait Provider: Send + Sync {
    fn engine(&self) -> Engine;
    fn binary(&self) -> &OsStr;
    fn default_model(&self) -> &str;
    fn profiles(&self) -> Result<Vec<ProviderProfile>>;
    fn auth_state(&self, profile: &ProviderProfile) -> AuthState;
    fn worker_command(&self, context: &WorkerLaunchContext) -> Result<ProviderCommand>;
    fn orchestrator_command(&self, request: &OrchestratorRequest) -> Result<ProviderCommand> {
        orchestrator::build(self.engine(), self.binary(), request)
    }
    fn orchestrator_bootstrap_command(
        &self,
        request: &OrchestratorRequest,
    ) -> Result<Option<ProviderCommand>> {
        orchestrator::build_bootstrap(self.engine(), self.binary(), request)
    }
    fn parse_events(&self, bytes: &[u8]) -> Result<ParsedEvents>;
    fn refresh_quota(
        &self,
        _profile: &Path,
        _session_id: Option<&str>,
        _observed_at: f64,
    ) -> Result<Option<CodexQuotaRefreshResult>> {
        Ok(None)
    }
}

pub struct ProviderRegistry {
    providers: BTreeMap<Engine, Box<dyn Provider>>,
    catalog: Option<catalog::CatalogSnapshot>,
    process_secrets: BTreeMap<(Engine, PathBuf), ProviderProcessSecret>,
}

impl ProviderRegistry {
    pub fn new(providers: impl IntoIterator<Item = Box<dyn Provider>>) -> Self {
        let providers = providers
            .into_iter()
            .map(|provider| (provider.engine(), provider))
            .collect();
        Self {
            providers,
            catalog: None,
            process_secrets: BTreeMap::new(),
        }
    }

    pub fn get(&self, engine: Engine) -> Option<&dyn Provider> {
        self.providers.get(&engine).map(Box::as_ref)
    }

    pub fn standard() -> Self {
        Self::new([
            Box::new(Claude::new(catalog::current_binary(Engine::Claude))) as Box<dyn Provider>,
            Box::new(Codex::new(catalog::current_binary(Engine::Codex))),
            Box::new(OpenCode::new(catalog::current_binary(Engine::Opencode))),
            Box::new(Kimi::new(catalog::current_binary(Engine::Kimi))),
            Box::new(Grok::new(catalog::current_binary(Engine::Grok))),
        ])
    }

    pub fn standard_with_catalog(snapshot: catalog::CatalogSnapshot) -> Self {
        Self::new([
            Box::new(Claude::new(snapshot_binary(&snapshot, Engine::Claude))) as Box<dyn Provider>,
            Box::new(Codex::new(snapshot_binary(&snapshot, Engine::Codex))),
            Box::new(OpenCode::new(snapshot_binary(&snapshot, Engine::Opencode))),
            Box::new(Kimi::new(snapshot_binary(&snapshot, Engine::Kimi))),
            Box::new(Grok::new(snapshot_binary(&snapshot, Engine::Grok))),
        ])
        .with_catalog(snapshot)
    }

    pub fn from_discovery(discovery: &catalog::ProviderDiscovery<'_>) -> Result<Self> {
        let snapshot = discovery.discover_all()?;
        let process_secrets = process_secret::from_catalog(&snapshot, discovery.environment)
            .into_iter()
            .collect();
        let mut registry = Self::standard_with_catalog(snapshot);
        registry.process_secrets = process_secrets;
        Ok(registry)
    }

    pub fn with_catalog(mut self, snapshot: catalog::CatalogSnapshot) -> Self {
        self.catalog = Some(snapshot);
        self
    }

    pub fn process_secret_for(&self, profile: &ProviderProfile) -> Option<ProviderProcessSecret> {
        self.process_secrets
            .get(&(profile.engine, profile.path.clone()))
            .cloned()
    }

    pub fn catalog(&self) -> Option<&catalog::CatalogSnapshot> {
        self.catalog.as_ref()
    }

    pub fn profiles_for(&self, engine: Engine) -> Result<Vec<ProviderProfile>> {
        if let Some(snapshot) = self.catalog.as_ref() {
            if let Some(provider) = snapshot.providers.get(&engine) {
                return Ok(provider.profiles.iter().map(provider_profile).collect());
            }
        }
        self.get(engine)
            .ok_or_else(|| {
                crate::Error::InvalidArgument(format!("provider unavailable: {engine}"))
            })?
            .profiles()
    }

    pub fn profile_eligibility(
        &self,
        profile: &ProviderProfile,
    ) -> Option<catalog::ProfileEligibility> {
        self.catalog
            .as_ref()?
            .providers
            .get(&profile.engine)?
            .profiles
            .iter()
            .find(|candidate| candidate.path == profile.path)
            .map(|candidate| candidate.eligibility)
    }

    pub fn managed_pool_eligible(&self, profile: &ProviderProfile) -> bool {
        self.profile_eligibility(profile).map_or_else(
            || {
                self.get(profile.engine).is_some_and(|provider| {
                    provider.auth_state(profile) == AuthState::Authenticated
                })
            },
            |eligibility| eligibility.managed_pool_eligible,
        )
    }

    pub fn rotation_eligible(&self, profile: &ProviderProfile) -> bool {
        self.profile_eligibility(profile)
            .map(|eligibility| eligibility.rotation_eligible)
            .unwrap_or_else(|| {
                // Without a discovered catalog, the provider adapter can only
                // establish engine-level support. Catalog-backed API-key
                // profiles are handled by the branch above and fail closed.
                matches!(profile.engine, Engine::Claude | Engine::Codex)
                    && self.get(profile.engine).is_some_and(|provider| {
                        provider.auth_state(profile) == AuthState::Authenticated
                    })
            })
    }

    pub fn worker_eligible(&self, profile: &ProviderProfile) -> bool {
        self.profile_eligibility(profile).map_or_else(
            || !profile.reserved && self.managed_pool_eligible(profile),
            |eligibility| eligibility.worker_eligible,
        )
    }

    pub fn orchestrator_eligible(&self, engine: Engine) -> bool {
        self.catalog
            .as_ref()
            .and_then(|snapshot| snapshot.providers.get(&engine))
            .map_or_else(
                || {
                    self.get(engine).is_some_and(|provider| {
                        provider.profiles().is_ok_and(|profiles| {
                            profiles.iter().any(|profile| {
                                provider.auth_state(profile) == AuthState::Authenticated
                            })
                        })
                    })
                },
                catalog::ProviderSnapshot::eligible_for_orchestrator,
            )
    }

    /// Returns the executable availability captured by discovery.
    ///
    /// Registries assembled directly from an injected provider adapter do not
    /// have a catalog. Those adapters are test/composition fixtures and their
    /// command is treated as available; process discovery always supplies the
    /// catalog-backed, fail-closed result.
    pub fn binary_available(&self, engine: Engine) -> bool {
        self.catalog
            .as_ref()
            .and_then(|snapshot| snapshot.providers.get(&engine))
            .map(|provider| provider.binary.available)
            .unwrap_or_else(|| self.get(engine).is_some())
    }

    pub fn engines(&self) -> impl Iterator<Item = Engine> + '_ {
        self.providers.keys().copied()
    }
}

fn provider_profile(profile: &catalog::ProfileSnapshot) -> ProviderProfile {
    ProviderProfile {
        engine: profile.engine,
        account: profile.account.clone(),
        path: profile.path.clone(),
        reserved: profile.reserved,
    }
}

fn snapshot_binary(snapshot: &catalog::CatalogSnapshot, engine: Engine) -> String {
    snapshot
        .providers
        .get(&engine)
        .map(|provider| provider.binary.program.clone())
        .unwrap_or_else(|| catalog::current_binary(engine))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn standard_registry_exposes_every_provider_adapter() {
        let registry = ProviderRegistry::standard();
        assert_eq!(registry.engines().collect::<Vec<_>>(), Engine::ALL);
    }

    #[test]
    fn injected_catalog_controls_profile_and_pool_eligibility() {
        let spec = catalog::spec(Engine::Kimi);
        let snapshot = catalog::CatalogSnapshot {
            providers: BTreeMap::from([(
                Engine::Kimi,
                catalog::ProviderSnapshot {
                    spec,
                    binary: catalog::BinaryStatus {
                        program: "kimi".into(),
                        available: true,
                        version: Some("fixture".into()),
                    },
                    profiles: vec![catalog::ProfileSnapshot {
                        engine: Engine::Kimi,
                        account: "api-key".into(),
                        path: PathBuf::from("/profiles/kimi-api-key"),
                        reserved: false,
                        auth: catalog::AuthStatus::Authenticated {
                            methods: vec![catalog::AuthMethod::ApiKey],
                        },
                        eligibility: catalog::ProfileEligibility {
                            credential_present: true,
                            authenticated: true,
                            worker_eligible: true,
                            orchestrator_eligible: true,
                            rotation_eligible: false,
                            managed_pool_eligible: true,
                        },
                    }],
                    models: vec![catalog::KIMI_DEFAULT_MODEL.into()],
                },
            )]),
        };
        let registry = ProviderRegistry::standard_with_catalog(snapshot);
        let profile = registry.profiles_for(Engine::Kimi).unwrap().remove(0);
        assert!(registry.worker_eligible(&profile));
        assert!(registry.managed_pool_eligible(&profile));
        assert!(!registry.rotation_eligible(&profile));
        assert!(registry.orchestrator_eligible(Engine::Kimi));
    }

    #[test]
    fn every_provider_plan_command_uses_its_read_only_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let registry = ProviderRegistry::standard();

        for engine in Engine::ALL {
            let profile = ProviderProfile {
                engine,
                account: "fixture".into(),
                path: temp.path().join(format!("{engine}-profile")),
                reserved: false,
            };
            let mut request = WorkerRequest::new(profile, temp.path(), "inspect the checkout");
            request.plan = true;
            if engine == Engine::Kimi {
                let kimi_plan_home = temp.path().join("kimi-plan-home");
                std::fs::create_dir_all(&kimi_plan_home).unwrap();
                request.config_home_override = Some(kimi_plan_home);
            }
            let context = WorkerLaunchContext::for_test(request);
            let command = registry
                .get(engine)
                .expect("standard registry provider")
                .worker_command(&context)
                .unwrap();
            let args = command.args_lossy();

            assert_eq!(command.cwd, temp.path(), "{engine} changed the checkout");
            match engine {
                Engine::Claude => {
                    assert_eq!(argument_value(&args, "--permission-mode"), Some("plan"));
                    assert!(!args.contains(&"--dangerously-skip-permissions".into()));
                }
                Engine::Codex => {
                    assert_eq!(argument_value(&args, "-s"), Some("read-only"));
                    assert!(!args.contains(&"--dangerously-bypass-approvals-and-sandbox".into()));
                }
                Engine::Opencode => {
                    assert_eq!(argument_value(&args, "--agent"), Some("plan"));
                    assert!(!args.contains(&"--auto".into()));
                }
                Engine::Kimi => {
                    assert!(
                        args.iter()
                            .any(|argument| argument.contains("READ-ONLY PLAN SCOUT"))
                    );
                    assert!(!args.contains(&"--auto".into()));
                    let expected_home = temp.path().join("kimi-plan-home").into_os_string();
                    assert_eq!(
                        command.env.get(std::ffi::OsStr::new("KIMI_CODE_HOME")),
                        Some(&expected_home)
                    );
                }
                Engine::Grok => {
                    assert_eq!(argument_value(&args, "--permission-mode"), Some("plan"));
                    assert!(!args.contains(&"--always-approve".into()));
                }
            }
        }
    }

    fn argument_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter()
            .position(|argument| argument == flag)
            .and_then(|index| args.get(index + 1))
            .map(String::as_str)
    }
}
