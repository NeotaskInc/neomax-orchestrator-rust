use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use neomax_core::config::Engine;
use neomax_core::providers::catalog::{
    CatalogSnapshot, MapEnvironment, RealFileSystem, discover_profile_snapshots,
};

use crate::config::AgentPaths;

#[derive(Debug, Clone, Default)]
pub(crate) struct ProfileCatalog {
    by_engine: BTreeMap<Engine, Vec<PathBuf>>,
}

impl ProfileCatalog {
    pub fn from_catalog(snapshot: &CatalogSnapshot) -> Self {
        let by_engine = Engine::ALL
            .into_iter()
            .map(|engine| {
                let profiles = snapshot
                    .providers
                    .get(&engine)
                    .map(|provider| {
                        provider
                            .profiles
                            .iter()
                            .map(|profile| profile.path.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                (engine, profiles)
            })
            .collect();
        Self { by_engine }
    }

    pub fn discover(paths: &AgentPaths) -> Self {
        if let Some(snapshot) = paths.provider_catalog() {
            return Self::from_catalog(snapshot);
        }

        let environment = MapEnvironment::new(env::vars())
            .with_home(paths.home.clone())
            .with_current_dir(env::current_dir().unwrap_or_else(|_| paths.home.clone()));
        let filesystem = RealFileSystem;
        let by_engine = Engine::ALL
            .into_iter()
            .map(|engine| {
                let profiles = discover_profile_snapshots(engine, &environment, &filesystem)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|profile| profile.path)
                    .collect();
                (engine, profiles)
            })
            .collect();
        Self { by_engine }
    }

    pub fn for_engine(&self, engine: Engine) -> &[PathBuf] {
        self.by_engine
            .get(&engine)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

pub(crate) fn profile_engine_paths(
    catalog: &ProfileCatalog,
) -> impl Iterator<Item = (Engine, &Path)> {
    Engine::ALL.into_iter().flat_map(|engine| {
        catalog
            .for_engine(engine)
            .iter()
            .map(move |path| (engine, path.as_path()))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use neomax_core::providers::catalog::{
        BinaryStatus, CatalogSnapshot, MapEnvironment, ProfileEligibility, ProfileSnapshot,
        ProviderSnapshot, RealFileSystem, discover_profile_snapshots, spec,
    };

    use super::*;

    #[test]
    fn snapshot_profiles_are_the_catalog_source() {
        let profile = PathBuf::from("/fixture/.kimi-code");
        let snapshot = CatalogSnapshot {
            providers: BTreeMap::from([(
                Engine::Kimi,
                ProviderSnapshot {
                    spec: spec(Engine::Kimi),
                    binary: BinaryStatus {
                        program: "kimi".into(),
                        available: true,
                        version: Some("fixture".into()),
                    },
                    profiles: vec![ProfileSnapshot {
                        engine: Engine::Kimi,
                        account: "default".into(),
                        path: profile.clone(),
                        reserved: false,
                        auth: neomax_core::providers::catalog::AuthStatus::Unauthenticated,
                        eligibility: ProfileEligibility::disconnected(),
                    }],
                    models: vec!["kimi-code/k3".into()],
                },
            )]),
        };
        let profiles = ProfileCatalog::from_catalog(&snapshot);
        assert_eq!(profiles.for_engine(Engine::Kimi), &[profile]);
        assert!(profiles.for_engine(Engine::Claude).is_empty());
    }

    #[test]
    fn canonical_catalog_order_is_preserved_for_every_engine() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        for engine in Engine::ALL {
            let provider = spec(engine);
            for suffix in [
                provider.default_profile_dir.clone(),
                format!("{}2", provider.account_prefix),
                format!("{}1", provider.account_prefix),
            ] {
                std::fs::create_dir_all(home.join(suffix)).unwrap();
            }
        }
        let environment = MapEnvironment::new(std::iter::empty())
            .with_home(home)
            .with_current_dir(home);
        let filesystem = RealFileSystem;
        let providers = Engine::ALL
            .into_iter()
            .map(|engine| {
                let profiles =
                    discover_profile_snapshots(engine, &environment, &filesystem).unwrap();
                let expected = profiles
                    .iter()
                    .map(|profile| profile.path.clone())
                    .collect::<Vec<_>>();
                (
                    engine,
                    (
                        ProviderSnapshot {
                            spec: spec(engine),
                            binary: BinaryStatus {
                                program: String::new(),
                                available: false,
                                version: None,
                            },
                            profiles,
                            models: Vec::new(),
                        },
                        expected,
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let snapshot = CatalogSnapshot {
            providers: providers
                .iter()
                .map(|(engine, (provider, _))| (*engine, provider.clone()))
                .collect(),
        };
        let catalog = ProfileCatalog::from_catalog(&snapshot);

        for engine in Engine::ALL {
            assert_eq!(catalog.for_engine(engine), providers[&engine].1.as_slice());
        }
    }

    #[test]
    fn explicit_profile_overrides_keep_canonical_order_for_every_engine() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let current_dir = home.join("project");
        std::fs::create_dir_all(&current_dir).unwrap();
        let mut values = BTreeMap::new();
        let mut expected = BTreeMap::new();
        for engine in Engine::ALL {
            let provider = spec(engine);
            let first = home.join(format!("{}-first", engine.as_str()));
            let second = home.join(format!("{}-second", engine.as_str()));
            std::fs::create_dir_all(&first).unwrap();
            std::fs::create_dir_all(&second).unwrap();
            values.insert(
                provider.profile_env,
                std::env::join_paths([first.clone(), second.clone()])
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
            expected.insert(
                engine,
                vec![
                    std::fs::canonicalize(first).unwrap(),
                    std::fs::canonicalize(second).unwrap(),
                ],
            );
        }
        let environment = MapEnvironment::new(values)
            .with_home(home)
            .with_current_dir(&current_dir);
        let filesystem = RealFileSystem;
        let snapshot = CatalogSnapshot {
            providers: Engine::ALL
                .into_iter()
                .map(|engine| {
                    let profiles =
                        discover_profile_snapshots(engine, &environment, &filesystem).unwrap();
                    (
                        engine,
                        ProviderSnapshot {
                            spec: spec(engine),
                            binary: BinaryStatus {
                                program: String::new(),
                                available: false,
                                version: None,
                            },
                            profiles,
                            models: Vec::new(),
                        },
                    )
                })
                .collect(),
        };
        let catalog = ProfileCatalog::from_catalog(&snapshot);
        for engine in Engine::ALL {
            assert_eq!(catalog.for_engine(engine), expected[&engine].as_slice());
        }
    }
}
