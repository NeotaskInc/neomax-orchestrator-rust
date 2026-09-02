use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use neomax_core::config::Engine;
use neomax_core::providers::{ProviderProfile, catalog};
use neomax_core::runs::RunRecord;
use neomax_core::sessions::SessionRecord;
use neomax_core::usage::UsageCacheStore;
use serde_json::json;

use super::capabilities::capabilities_for;
use super::deduplication::mark_duplicate_accounts;
use super::presentation::account_view;
use super::profile::AccountContext;
use super::state::ControlState;
use crate::model::EngineView;
use crate::source::FilesystemPortalSource;

/// Discover every configured provider profile and turn each one into a safe
/// portal view. Discovery owns enumeration; profile inspection and rendering
/// live in their dedicated modules.
pub(crate) fn account_views(
    source: &FilesystemPortalSource,
    runs: &[RunRecord],
    sessions: &[SessionRecord],
    now: i64,
    session_window_days: u32,
) -> Result<BTreeMap<String, EngineView>> {
    let controls = ControlState::load(&source.paths.cooldowns, &source.paths.paused);
    let usage = UsageCacheStore::new(source.paths.usage.clone());
    let probe = neomax_core::runs::SystemProcessProbe;
    let when = Utc.timestamp_opt(now, 0).single().unwrap_or_else(Utc::now);
    let context = AccountContext {
        home: &source.home,
        environment: &source.discovery_environment,
        controls: &controls,
        usage: &usage,
        runs,
        sessions,
        probe: &probe,
        now: when,
        session_window_days,
    };
    let mut output = BTreeMap::new();

    for engine in Engine::ALL {
        let binary_available = source
            .provider_snapshot(engine)
            .is_some_and(|snapshot| snapshot.binary.available);
        let profiles = if let Some(snapshot) = source.provider_snapshot(engine) {
            snapshot
                .profiles
                .iter()
                .map(|profile| {
                    (
                        ProviderProfile {
                            engine: profile.engine,
                            account: profile.account.clone(),
                            path: profile.path.clone(),
                            reserved: profile.reserved,
                        },
                        Some(profile.eligibility),
                        match &profile.auth {
                            catalog::AuthStatus::Authenticated { methods } => {
                                Some(methods.to_vec())
                            }
                            _ => None,
                        },
                    )
                })
                .collect::<Vec<_>>()
        } else {
            catalog::discover_profile_snapshots(
                engine,
                &source.discovery_environment,
                &catalog::RealFileSystem,
            )?
            .into_iter()
            .map(|snapshot| {
                let auth_methods = match &snapshot.auth {
                    catalog::AuthStatus::Authenticated { methods } => Some(methods.clone()),
                    _ => None,
                };
                (
                    ProviderProfile {
                        engine: snapshot.engine,
                        account: snapshot.account,
                        path: snapshot.path,
                        reserved: snapshot.reserved,
                    },
                    Some(snapshot.eligibility),
                    auth_methods,
                )
            })
            .collect::<Vec<_>>()
        };
        let mut accounts = profiles
            .iter()
            .map(|(profile, eligibility, auth_methods)| {
                account_view(
                    profile,
                    engine,
                    *eligibility,
                    auth_methods.as_deref(),
                    binary_available,
                    &context,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        mark_duplicate_accounts(&mut accounts);
        let capabilities = accounts
            .first()
            .map(|account| account.capabilities.clone())
            .unwrap_or_else(|| {
                capabilities_for(
                    engine,
                    &source.home,
                    &None,
                    &json!({}),
                    &source.discovery_environment,
                )
            });
        output.insert(
            engine.as_str().to_string(),
            EngineView {
                accounts,
                capabilities,
            },
        );
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use neomax_core::providers::catalog::{
        AuthMethod, AuthStatus, BinaryStatus, CatalogSnapshot, ProfileEligibility, ProfileSnapshot,
        ProviderSnapshot, spec,
    };

    use super::*;

    #[test]
    fn account_views_use_the_injected_catalog_eligibility() {
        let source = FilesystemPortalSource::new("/fixture/home", "/fixture/state").with_catalog(
            CatalogSnapshot {
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
                            account: "1".into(),
                            path: PathBuf::from("/fixture/kimi-profile"),
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
                        }],
                        models: vec!["kimi-code/k3".into()],
                    },
                )]),
            },
        );
        let views = account_views(&source, &[], &[], 1_800_000_000, 30).unwrap();
        let kimi = &views["kimi"].accounts;
        assert_eq!(kimi.len(), 1);
        assert!(kimi[0].authenticated);
        assert!(kimi[0].worker_eligible);
        assert_eq!(kimi[0].auth_method.as_deref(), Some("API key"));
        assert!(kimi[0].eligibility.credential_present);
        assert!(kimi[0].eligibility.orchestrator_eligible);
        assert!(!kimi[0].eligibility.rotation_eligible);
        assert!(kimi[0].eligibility.managed_pool_eligible);
    }

    #[test]
    fn account_views_keep_authenticated_profiles_when_the_binary_is_missing() {
        let source = FilesystemPortalSource::new("/fixture/home", "/fixture/state").with_catalog(
            CatalogSnapshot {
                providers: BTreeMap::from([(
                    Engine::Kimi,
                    ProviderSnapshot {
                        spec: spec(Engine::Kimi),
                        binary: BinaryStatus {
                            program: "kimi".into(),
                            available: false,
                            version: None,
                        },
                        profiles: vec![ProfileSnapshot {
                            engine: Engine::Kimi,
                            account: "1".into(),
                            path: PathBuf::from("/fixture/kimi-profile"),
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
                        }],
                        models: vec!["kimi-code/k3".into()],
                    },
                )]),
            },
        );
        let views = account_views(&source, &[], &[], 1_800_000_000, 30).unwrap();
        let kimi = &views["kimi"].accounts;
        assert_eq!(kimi.len(), 1);
        assert!(kimi[0].authenticated);
        assert!(!kimi[0].worker_eligible);
        assert!(!kimi[0].eligibility.orchestrator_eligible);
        assert!(kimi[0].eligibility.managed_pool_eligible);
    }

    #[cfg(unix)]
    #[test]
    fn fallback_discovery_uses_poisoned_environment_without_spawning_commands() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        use neomax_core::providers::catalog::MapEnvironment;

        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let security = bin.join("security");
        fs::write(
            &security,
            "#!/bin/sh\ntouch \"$(dirname \"$0\")/invoked\"\nexit 0\n",
        )
        .unwrap();
        fs::set_permissions(&security, fs::Permissions::from_mode(0o755)).unwrap();

        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let environment = MapEnvironment::new([
            ("PATH".to_string(), bin.to_string_lossy().into_owned()),
            ("NEOMAX_POISON".to_string(), "fixture-only".to_string()),
        ])
        .with_home(home.clone())
        .with_current_dir(temp.path());
        let source = FilesystemPortalSource::new(&home, temp.path().join("state"))
            .with_discovery_environment(environment);

        let views = account_views(&source, &[], &[], 1_800_000_000, 30).unwrap();

        assert!(views.contains_key("claude"));
        assert!(!bin.join("invoked").exists());
    }

    #[test]
    fn empty_provider_profiles_still_publish_catalog_capabilities_for_all_engines() {
        let source = FilesystemPortalSource::new("/fixture/home", "/fixture/state");
        let views = account_views(&source, &[], &[], 1_800_000_000, 30).unwrap();
        assert_eq!(views.len(), Engine::ALL.len());
        for engine in Engine::ALL {
            assert!(views.contains_key(engine.as_str()));
        }
        for engine in [Engine::Opencode, Engine::Kimi, Engine::Grok] {
            assert!(views[engine.as_str()].capabilities.quota.reactive);
            assert!(!views[engine.as_str()].capabilities.quota.supported);
        }
    }
}
