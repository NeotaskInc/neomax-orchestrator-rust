use std::collections::BTreeMap;

use crate::{Engine, Result};

use super::commands::{CommandRunner, binary_status, model_ids};
#[cfg(target_os = "macos")]
use super::commands::DiscoveryCommand;
use super::environment::Environment;
use super::filesystem::FileSystem;
use super::profiles::discover_profile_snapshots;
use super::specs::{all_specs, spec};
use super::types::{CatalogSnapshot, ProviderSnapshot};
#[cfg(target_os = "macos")]
use super::types::{AuthMethod, AuthStatus, ProfileSnapshot};

pub struct ProviderDiscovery<'a> {
    pub environment: &'a dyn Environment,
    pub filesystem: &'a dyn FileSystem,
    pub commands: &'a dyn CommandRunner,
}

impl<'a> ProviderDiscovery<'a> {
    pub fn discover(&self, engine: Engine) -> Result<ProviderSnapshot> {
        let provider = spec(engine);
        let profiles = discover_profile_snapshots(engine, self.environment, self.filesystem)?;
        #[cfg(target_os = "macos")]
        let profiles = {
            let mut profiles = profiles;
            if engine == Engine::Claude {
                self.merge_claude_keychain_profiles(&mut profiles);
            }
            profiles
        };
        Ok(ProviderSnapshot {
            spec: provider,
            binary: binary_status(engine, self.environment, self.commands),
            profiles,
            models: model_ids(engine, self.environment, self.commands),
        })
    }

    pub fn discover_all(&self) -> Result<CatalogSnapshot> {
        let providers = all_specs()
            .map(|provider| {
                self.discover(provider.engine)
                    .map(|item| (provider.engine, item))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        Ok(CatalogSnapshot { providers })
    }

    #[cfg(target_os = "macos")]
    fn merge_claude_keychain_profiles(&self, profiles: &mut [ProfileSnapshot]) {
        let Some(home) = self.environment.home_dir() else {
            return;
        };
        for profile in profiles {
            if profile.auth.is_authenticated() {
                continue;
            }
            let command = DiscoveryCommand {
                program: "security".into(),
                args: vec![
                    "find-generic-password".into(),
                    "-s".into(),
                    match super::profiles::checked_claude_keychain_service(&profile.path, &home) {
                        Ok(service) => service,
                        Err(_) => continue,
                    },
                ],
                cwd: Some(self.environment.current_dir()),
                safe_environment: self
                    .environment
                    .value("PATH")
                    .map(|path| BTreeMap::from([(String::from("PATH"), path)]))
                    .unwrap_or_default(),
            };
            let Ok(output) = self.commands.run(&command) else {
                continue;
            };
            if output.success && !output.timed_out && !output.truncated {
                profile.auth = AuthStatus::Authenticated {
                    methods: vec![AuthMethod::OAuth],
                };
                profile.eligibility.credential_present = true;
                profile.eligibility.authenticated = true;
                profile.eligibility.worker_eligible = !profile.reserved;
                profile.eligibility.orchestrator_eligible = true;
                profile.eligibility.rotation_eligible = true;
                profile.eligibility.managed_pool_eligible = true;
            }
        }
    }
}
