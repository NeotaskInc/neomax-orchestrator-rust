use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use neomax_core::{EffectiveSettings, Engine, SettingsFile, StatePaths};

use super::super::process::{ProcessInvocation, ProcessOutcome, ProcessPort};
use super::super::profiles::{AuthPort, DetectedAuth, ManagedProfile};
use super::super::request::{AccountSelector, AuthMode};

pub(super) struct FakeAuth {
    pub(super) profiles: Vec<ManagedProfile>,
    pub(super) ensured: Arc<Mutex<Vec<String>>>,
}

impl AuthPort for FakeAuth {
    fn profiles(
        &self,
        _engine: Engine,
        _home: &Path,
        _cwd: &Path,
    ) -> anyhow::Result<Vec<ManagedProfile>> {
        Ok(self.profiles.clone())
    }

    fn ensure_profile(
        &self,
        engine: Engine,
        account: &AccountSelector,
        _home: &Path,
        _cwd: &Path,
    ) -> anyhow::Result<ManagedProfile> {
        self.ensured.lock().unwrap().push(account.label());
        self.profiles
            .iter()
            .find(|profile| profile.account() == account.label())
            .cloned()
            .or_else(|| {
                Some(ManagedProfile {
                    profile: neomax_core::providers::ProviderProfile {
                        engine,
                        account: account.label(),
                        path: PathBuf::from("/fixture/new-profile"),
                        reserved: false,
                    },
                    auth: None,
                })
            })
            .ok_or_else(|| anyhow::anyhow!("missing fixture profile"))
    }

    fn api_key(&self, _engine: Engine) -> anyhow::Result<String> {
        Ok("fixture-api-key".into())
    }

    fn choose_auth_mode(&self, _engine: Engine) -> anyhow::Result<AuthMode> {
        Ok(AuthMode::ApiKey)
    }

    fn configure_api_key(
        &self,
        engine: Engine,
        account: &AccountSelector,
        _home: &Path,
        _cwd: &Path,
        _secret: &str,
    ) -> anyhow::Result<ManagedProfile> {
        Ok(ManagedProfile {
            profile: neomax_core::providers::ProviderProfile {
                engine,
                account: account.label(),
                path: PathBuf::from("/fixture/grok/api-key"),
                reserved: false,
            },
            auth: Some(DetectedAuth::ApiKey),
        })
    }
}

pub(super) struct FakeProcess {
    pub(super) calls: Mutex<Vec<ProcessInvocation>>,
    outcome: ProcessOutcome,
}

impl FakeProcess {
    pub(super) fn successful() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            outcome: ProcessOutcome {
                status_code: Some(0),
                success: true,
                stdout: b"fixture output\n".to_vec(),
                stderr: Vec::new(),
            },
        }
    }
}

impl ProcessPort for FakeProcess {
    fn invoke(&self, request: &ProcessInvocation) -> anyhow::Result<ProcessOutcome> {
        self.calls.lock().unwrap().push(request.clone());
        Ok(self.outcome.clone())
    }
}

pub(super) fn context() -> (tempfile::TempDir, crate::context::RuntimeContext) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let workspace = temp.path().join("workspace");
    let state = temp.path().join("state");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&workspace).unwrap();
    let paths = StatePaths::new(home, state);
    paths.ensure_runtime_dirs().unwrap();
    let settings = EffectiveSettings::resolve(
        SettingsFile::default(),
        paths.state.join("config.toml"),
        &std::collections::BTreeMap::new(),
    )
    .unwrap();
    (
        temp,
        crate::context::RuntimeContext::for_test(
            paths,
            settings,
            workspace,
            1_700_000_000,
            neomax_core::orchestration::registry::OrchestratorLiveness::default(),
            None,
        ),
    )
}

pub(super) fn profile(engine: Engine, account: &str, auth: Option<DetectedAuth>) -> ManagedProfile {
    ManagedProfile {
        profile: neomax_core::providers::ProviderProfile {
            engine,
            account: account.into(),
            path: PathBuf::from(format!("/fixture/{engine}/{account}")),
            reserved: false,
        },
        auth,
    }
}

pub(super) fn provider_profile(
    engine: Engine,
    account: &str,
    path: PathBuf,
) -> neomax_core::providers::ProviderProfile {
    neomax_core::providers::ProviderProfile {
        engine,
        account: account.into(),
        path,
        reserved: false,
    }
}

#[test]
fn fake_auth_new_profiles_keep_the_requested_engine() {
    let auth = FakeAuth {
        profiles: Vec::new(),
        ensured: Arc::new(Mutex::new(Vec::new())),
    };
    let profile = auth
        .ensure_profile(
            Engine::Grok,
            &AccountSelector::Number(7),
            Path::new("/fixture/home"),
            Path::new("/fixture/workspace"),
        )
        .unwrap();
    assert_eq!(profile.profile.engine, Engine::Grok);
}
