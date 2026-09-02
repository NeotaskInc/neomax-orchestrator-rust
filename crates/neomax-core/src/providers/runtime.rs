use std::sync::Arc;

use crate::Result;

use super::ProviderRegistry;
use super::catalog::{
    BinaryStatus, CatalogSnapshot, LocalCommandRunner, ProcessEnvironment, ProviderDiscovery,
    ProviderSnapshot, RealFileSystem, all_specs,
};

/// The immutable provider view shared by one Neomax process.
///
/// Composition roots build this once and pass the same snapshot to routing,
/// account views, usage collection, and provider execution. The catalog
/// contains metadata and credential state only. Environment credentials, when
/// selected, remain in a non-serializable registry sidecar for final process
/// injection and never enter a run, portal, or agent environment.
#[derive(Clone)]
pub struct ProviderRuntime {
    catalog: Arc<CatalogSnapshot>,
    registry: Arc<ProviderRegistry>,
}

impl ProviderRuntime {
    pub fn from_discovery(discovery: &ProviderDiscovery<'_>) -> Result<Self> {
        let registry = ProviderRegistry::from_discovery(discovery)?;
        let catalog = registry.catalog().cloned().ok_or_else(|| {
            crate::Error::Message("provider discovery returned no catalog".into())
        })?;
        Ok(Self {
            catalog: Arc::new(catalog),
            registry: Arc::new(registry),
        })
    }

    pub fn discover_process() -> Result<Self> {
        let environment = ProcessEnvironment;
        let filesystem = RealFileSystem;
        let commands = LocalCommandRunner::default();
        let discovery = ProviderDiscovery {
            environment: &environment,
            filesystem: &filesystem,
            commands: &commands,
        };
        Self::from_discovery(&discovery)
    }

    pub fn from_catalog(catalog: CatalogSnapshot) -> Self {
        let registry = ProviderRegistry::standard_with_catalog(catalog.clone());
        Self {
            catalog: Arc::new(catalog),
            registry: Arc::new(registry),
        }
    }

    pub fn empty() -> Self {
        let catalog = CatalogSnapshot {
            providers: all_specs()
                .map(|provider| {
                    (
                        provider.engine,
                        ProviderSnapshot {
                            spec: provider,
                            binary: BinaryStatus {
                                program: String::new(),
                                available: false,
                                version: None,
                            },
                            profiles: Vec::new(),
                            models: Vec::new(),
                        },
                    )
                })
                .collect(),
        };
        Self::from_catalog(catalog)
    }

    pub fn catalog(&self) -> &CatalogSnapshot {
        self.catalog.as_ref()
    }

    pub fn registry(&self) -> &ProviderRegistry {
        self.registry.as_ref()
    }

    pub fn catalog_arc(&self) -> Arc<CatalogSnapshot> {
        Arc::clone(&self.catalog)
    }

    pub fn registry_arc(&self) -> Arc<ProviderRegistry> {
        Arc::clone(&self.registry)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::Engine;
    use crate::providers::catalog::{
        AuthMethod, AuthStatus, BinaryStatus, CommandOutput, CommandRunner, DiscoveryCommand,
        Environment, FileSystem, ProfileEligibility, ProfileSnapshot, ProviderCapabilities,
        ProviderSnapshot, ProviderSpec,
    };

    struct EmptyFileSystem;

    impl FileSystem for EmptyFileSystem {
        fn is_file(&self, _path: &std::path::Path) -> bool {
            false
        }

        fn is_dir(&self, _path: &std::path::Path) -> bool {
            false
        }

        fn read(&self, _path: &std::path::Path) -> crate::Result<Option<Vec<u8>>> {
            Ok(None)
        }

        fn children(&self, _path: &std::path::Path) -> crate::Result<Vec<PathBuf>> {
            Ok(Vec::new())
        }
    }

    struct EmptyCommands;

    impl CommandRunner for EmptyCommands {
        fn run(&self, _command: &DiscoveryCommand) -> crate::Result<CommandOutput> {
            Ok(CommandOutput {
                success: false,
                stdout: Vec::new(),
                timed_out: false,
                truncated: false,
            })
        }
    }

    struct FixtureEnvironment;

    impl Environment for FixtureEnvironment {
        fn value(&self, _key: &str) -> Option<String> {
            None
        }

        fn home_dir(&self) -> Option<PathBuf> {
            Some(PathBuf::from("/fixture/home"))
        }

        fn current_dir(&self) -> PathBuf {
            PathBuf::from("/fixture/project")
        }
    }

    fn provider_spec(engine: Engine) -> ProviderSpec {
        ProviderSpec {
            engine,
            default_binary: engine.as_str().into(),
            binary_env: format!("NEOMAX_{}_BIN", engine.as_str().to_ascii_uppercase()),
            config_env: format!("NEOMAX_{}_CONFIG", engine.as_str().to_ascii_uppercase()),
            profile_env: format!("NEOMAX_{}_PROFILES", engine.as_str().to_ascii_uppercase()),
            default_profile_dir: format!(".{}", engine.as_str()),
            account_prefix: format!(".{}-acct", engine.as_str()),
            orchestrator_dir: format!(".{}-orch", engine.as_str()),
            orchestrator_env: format!("NEOMAX_{}_ORCH", engine.as_str().to_ascii_uppercase()),
            model_env: format!("NEOMAX_{}_MODEL", engine.as_str().to_ascii_uppercase()),
            default_model: format!("{}/default", engine.as_str()),
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
                auth_methods: vec![AuthMethod::LocalCredential],
            },
        }
    }

    #[test]
    fn from_discovery_keeps_one_catalog_for_registry_and_consumers() {
        let environment = FixtureEnvironment;
        let filesystem = EmptyFileSystem;
        let commands = EmptyCommands;
        let discovery = ProviderDiscovery {
            environment: &environment,
            filesystem: &filesystem,
            commands: &commands,
        };
        let runtime = ProviderRuntime::from_discovery(&discovery).unwrap();
        assert!(runtime.catalog().providers.contains_key(&Engine::Claude));
        assert!(runtime.registry().catalog().is_some());
        assert_eq!(
            runtime.registry().catalog().unwrap().providers.len(),
            runtime.catalog().providers.len()
        );
    }

    #[test]
    fn from_catalog_preserves_profile_metadata_without_discovery() {
        let profile = ProfileSnapshot {
            engine: Engine::Kimi,
            account: "fixture".into(),
            path: PathBuf::from("/fixture/kimi"),
            reserved: false,
            auth: AuthStatus::Authenticated {
                methods: vec![AuthMethod::ApiKey],
            },
            eligibility: ProfileEligibility {
                credential_present: true,
                authenticated: true,
                worker_eligible: true,
                orchestrator_eligible: true,
                rotation_eligible: false,
                managed_pool_eligible: true,
            },
        };
        let catalog = CatalogSnapshot {
            providers: BTreeMap::from([(
                Engine::Kimi,
                ProviderSnapshot {
                    spec: provider_spec(Engine::Kimi),
                    binary: BinaryStatus {
                        program: "kimi-fixture".into(),
                        available: true,
                        version: Some("fixture".into()),
                    },
                    profiles: vec![profile],
                    models: vec!["kimi-code/k3".into()],
                },
            )]),
        };
        let runtime = ProviderRuntime::from_catalog(catalog);
        assert_eq!(runtime.catalog().providers[&Engine::Kimi].profiles.len(), 1);
        assert_eq!(
            runtime.registry().profiles_for(Engine::Kimi).unwrap().len(),
            1
        );
    }

    #[test]
    fn empty_runtime_has_all_provider_keys_without_filesystem_discovery() {
        let runtime = ProviderRuntime::empty();
        assert_eq!(runtime.catalog().providers.len(), Engine::ALL.len());
        assert!(
            runtime
                .catalog()
                .providers
                .values()
                .all(|provider| { !provider.binary.available && provider.profiles.is_empty() })
        );
    }
}
