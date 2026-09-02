use std::fmt;
use std::path::PathBuf;

use crate::Engine;

use super::catalog::{AuthMethod, AuthStatus, CatalogSnapshot, Environment};

pub(crate) const PROCESS_SECRET_BOUNDARY_ENV: &str = "NEOMAX_PROVIDER_SECRET_BOUNDARY";

const CLAUDE_KEYS: &[&str] = &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];
const CODEX_KEYS: &[&str] = &["OPENAI_API_KEY", "CODEX_API_KEY"];
const OPENCODE_KEYS: &[&str] = &["OPENCODE_API_KEY", "OPENCODE_ZEN_API_KEY", "OPENAI_API_KEY"];
const KIMI_KEYS: &[&str] = &[
    "KIMI_API_KEY",
    "KIMI_MODEL_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GOOGLE_API_KEY",
    "VERTEXAI_API_KEY",
];
const GROK_KEYS: &[&str] = &["XAI_API_KEY", "GROK_API_KEY", "GROK_DEPLOYMENT_KEY"];

/// A credential selected for one provider process.
///
/// The value is deliberately not serializable and its debug output is always
/// redacted. It must only be attached to the final provider command.
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderProcessSecret {
    engine: Engine,
    variable: String,
    value: String,
}

impl fmt::Debug for ProviderProcessSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderProcessSecret")
            .field("engine", &self.engine)
            .field("variable", &self.variable)
            .field("value", &"<redacted>")
            .finish()
    }
}

impl ProviderProcessSecret {
    fn new(engine: Engine, variable: &str, value: String) -> Option<Self> {
        (!value.trim().is_empty()).then(|| Self {
            engine,
            variable: variable.into(),
            value,
        })
    }

    pub(crate) fn engine(&self) -> Engine {
        self.engine
    }

    pub(crate) fn variable(&self) -> &str {
        &self.variable
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

pub(crate) fn supported_environment_keys(engine: Engine) -> &'static [&'static str] {
    match engine {
        Engine::Claude => CLAUDE_KEYS,
        Engine::Codex => CODEX_KEYS,
        Engine::Opencode => OPENCODE_KEYS,
        Engine::Kimi => KIMI_KEYS,
        Engine::Grok => GROK_KEYS,
    }
}

pub fn is_secret_environment_key(key: &str) -> bool {
    let normalized = key.to_ascii_uppercase();
    normalized.ends_with("_KEY")
        || normalized.ends_with("_TOKEN")
        || normalized.ends_with("_API_KEY")
        || normalized.ends_with("_AUTH_TOKEN")
        || normalized.ends_with("_OAUTH_TOKEN")
        || normalized.ends_with("_ACCESS_TOKEN")
        || normalized.ends_with("_REFRESH_TOKEN")
        || normalized.contains("SECRET")
        || normalized.contains("PASSWORD")
        || matches!(
            normalized.as_str(),
            "OPENCODE_AUTH_CONTENT"
                | "KIMI_MODEL_BASE_URL"
                | "KIMI_CODE_BASE_URL"
                | "KIMI_BASE_URL"
        )
}

/// Remove ambient provider credentials and profile selectors before a local
/// helper or provider child receives its explicit environment.
pub fn scrub_provider_environment(command: &mut std::process::Command) {
    for (key, _) in std::env::vars_os() {
        if is_secret_environment_key(&key.to_string_lossy()) {
            command.env_remove(key);
        }
    }
    for engine in Engine::ALL {
        let spec = super::catalog::spec(engine);
        command.env_remove(spec.config_env);
        command.env_remove(spec.orchestrator_env);
        command.env_remove(spec.profile_env);
        for key in &spec.scrub {
            command.env_remove(key);
        }
    }
}

pub fn scrub_provider_process_request(
    mut request: crate::io::ProcessRequest,
) -> crate::io::ProcessRequest {
    for (key, _) in std::env::vars_os() {
        if is_secret_environment_key(&key.to_string_lossy()) {
            remove_process_request_key(&mut request, key);
        }
    }
    for engine in Engine::ALL {
        let spec = super::catalog::spec(engine);
        remove_process_request_key(&mut request, spec.config_env);
        remove_process_request_key(&mut request, spec.orchestrator_env);
        remove_process_request_key(&mut request, spec.profile_env);
        for key in &spec.scrub {
            remove_process_request_key(&mut request, key);
        }
    }
    request
}

fn remove_process_request_key(
    request: &mut crate::io::ProcessRequest,
    key: impl Into<std::ffi::OsString>,
) {
    let key = key.into();
    request.environment.remove(&key);
    request.remove_environment.insert(key);
}

pub(crate) fn process_secret_allowed(environment: &dyn Environment) -> bool {
    environment.value(PROCESS_SECRET_BOUNDARY_ENV).as_deref() != Some("1")
}

pub(crate) fn from_catalog(
    catalog: &CatalogSnapshot,
    environment: &dyn Environment,
) -> Vec<((Engine, PathBuf), ProviderProcessSecret)> {
    if !process_secret_allowed(environment) {
        return Vec::new();
    }
    catalog
        .providers
        .values()
        .flat_map(|provider| {
            provider.profiles.iter().filter_map(|profile| {
                let AuthStatus::Authenticated { methods } = &profile.auth else {
                    return None;
                };
                if profile.eligibility.credential_present
                    || !methods.contains(&AuthMethod::ApiKey)
                    || methods
                        .iter()
                        .any(|method| matches!(method, AuthMethod::OAuth | AuthMethod::Device))
                {
                    return None;
                }
                let secret = supported_environment_keys(profile.engine)
                    .iter()
                    .find_map(|key| {
                        environment.value(key).and_then(|value| {
                            ProviderProcessSecret::new(profile.engine, key, value)
                        })
                    })?;
                Some(((profile.engine, profile.path.clone()), secret))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use super::*;
    use crate::providers::catalog::{
        AuthMethod, BinaryStatus, CatalogSnapshot, MapEnvironment, ProfileEligibility,
        ProfileSnapshot, ProviderCapabilities, ProviderSnapshot, ProviderSpec,
    };

    fn snapshot(engine: Engine, home: &Path) -> CatalogSnapshot {
        let profile = home.join(format!(".{engine}"));
        let spec = ProviderSpec {
            engine,
            default_binary: engine.as_str().into(),
            binary_env: format!("NEOMAX_{}_BIN", engine.as_str().to_ascii_uppercase()),
            config_env: format!("NEOMAX_{}_HOME", engine.as_str().to_ascii_uppercase()),
            profile_env: format!("NEOMAX_{}_PROFILES", engine.as_str().to_ascii_uppercase()),
            default_profile_dir: format!(".{engine}"),
            account_prefix: format!(".{engine}-acct"),
            orchestrator_dir: format!(".{engine}-orch"),
            orchestrator_env: format!("NEOMAX_{}_ORCH", engine.as_str().to_ascii_uppercase()),
            model_env: format!("NEOMAX_{}_MODEL", engine.as_str().to_ascii_uppercase()),
            default_model: "fixture/model".into(),
            model_args: Vec::new(),
            default_unsets_config_env: false,
            scrub: Vec::new(),
            capabilities: ProviderCapabilities {
                orchestrator: true,
                worker: true,
                multiple_profiles: true,
                model_discovery: crate::providers::catalog::ModelDiscoverySupport::Unavailable,
                native_sessions: true,
                usage_discovery: true,
                auth_methods: vec![AuthMethod::ApiKey],
            },
        };
        CatalogSnapshot {
            providers: [(
                engine,
                ProviderSnapshot {
                    spec,
                    binary: BinaryStatus {
                        program: engine.as_str().into(),
                        available: true,
                        version: None,
                    },
                    profiles: vec![ProfileSnapshot {
                        engine,
                        account: "1".into(),
                        path: profile,
                        reserved: false,
                        auth: AuthStatus::Authenticated {
                            methods: vec![AuthMethod::ApiKey],
                        },
                        eligibility: ProfileEligibility {
                            credential_present: false,
                            authenticated: true,
                            worker_eligible: true,
                            orchestrator_eligible: true,
                            rotation_eligible: false,
                            managed_pool_eligible: true,
                        },
                    }],
                    models: Vec::new(),
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn secret_debug_output_is_redacted() {
        let secret =
            ProviderProcessSecret::new(Engine::Claude, "ANTHROPIC_API_KEY", "fixture".into())
                .unwrap();
        let debug = format!("{secret:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("fixture"));
    }

    #[test]
    fn each_provider_selects_only_its_first_supported_environment_key() {
        let home = Path::new("/fixture/home");
        for engine in Engine::ALL {
            let first = supported_environment_keys(engine)[0];
            let environment = MapEnvironment::new(
                supported_environment_keys(engine)
                    .iter()
                    .enumerate()
                    .map(|(index, key)| ((*key).into(), format!("fixture-{index}"))),
            );
            let catalog = snapshot(engine, home);
            let profile = &catalog.providers[&engine].profiles[0];
            let secrets = from_catalog(&catalog, &environment);
            assert_eq!(secrets.len(), 1);
            assert_eq!(secrets[0].1.variable(), first);
            assert!(!format!("{:?}", secrets[0].1).contains("fixture-"));
            assert_eq!(secrets[0].0, (engine, profile.path.clone()));
        }
    }

    #[test]
    fn provider_child_boundary_disables_environment_credential_selection() {
        let environment = MapEnvironment::new([
            ("ANTHROPIC_API_KEY".into(), "fixture-secret".into()),
            (PROCESS_SECRET_BOUNDARY_ENV.into(), "1".into()),
        ]);
        let catalog = snapshot(Engine::Claude, Path::new("/fixture/home"));
        assert!(from_catalog(&catalog, &environment).is_empty());
        assert!(!process_secret_allowed(&environment));
    }

    #[test]
    fn generic_local_process_requests_drop_provider_secrets_and_selectors() {
        let request = crate::io::ProcessRequest::new("fixture")
            .env("OPENAI_API_KEY", "fixture-secret")
            .env("CODEX_HOME", "/fixture/profile")
            .env("SAFE_FLAG", "kept");
        let request = scrub_provider_process_request(request);

        assert!(!request
            .environment
            .contains_key(OsStr::new("OPENAI_API_KEY")));
        assert!(!request.environment.contains_key(OsStr::new("CODEX_HOME")));
        assert!(request.environment.contains_key(OsStr::new("SAFE_FLAG")));
        assert!(request
            .remove_environment
            .iter()
            .any(|key| key == "OPENAI_API_KEY"));
        assert!(request
            .remove_environment
            .iter()
            .any(|key| key == "CODEX_HOME"));
    }
}
