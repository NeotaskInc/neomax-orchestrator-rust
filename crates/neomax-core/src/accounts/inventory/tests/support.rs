use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::PathBuf;

use crate::accounts::{LiveWorkSnapshot, LiveWorkSource, QuotaSnapshot, QuotaSnapshotSource};
use crate::providers::catalog::{
    AuthMethod, AuthStatus, BinaryStatus, CatalogSnapshot, ProfileEligibility, ProfileSnapshot,
    ProviderSnapshot, spec,
};
use crate::providers::runtime::ProviderRuntime;
use crate::providers::{AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile};
use crate::{Engine, Error, Result};

pub(super) struct ProviderFixture {
    pub(super) profiles: Vec<ProviderProfile>,
}

impl Provider for ProviderFixture {
    fn engine(&self) -> Engine {
        Engine::Codex
    }

    fn binary(&self) -> &OsStr {
        OsStr::new("fixture")
    }

    fn default_model(&self) -> &str {
        "model"
    }

    fn profiles(&self) -> Result<Vec<ProviderProfile>> {
        Ok(self.profiles.clone())
    }

    fn auth_state(&self, profile: &ProviderProfile) -> AuthState {
        if profile.account == "1" {
            AuthState::Authenticated
        } else {
            AuthState::Unauthenticated
        }
    }

    fn worker_command(
        &self,
        _context: &crate::providers::WorkerLaunchContext,
    ) -> Result<ProviderCommand> {
        Err(Error::Message("not used".into()))
    }

    fn parse_events(&self, _bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(ParsedEvents::default())
    }
}

pub(super) struct QuotaFixture {
    pub(super) snapshots: BTreeMap<(Engine, PathBuf), QuotaSnapshot>,
}

impl QuotaSnapshotSource for QuotaFixture {
    fn quota_snapshot(&self, engine: Engine, profile: &std::path::Path) -> QuotaSnapshot {
        self.snapshots
            .get(&(engine, profile.to_path_buf()))
            .cloned()
            .unwrap_or_default()
    }
}

pub(super) struct LiveWorkFixture {
    pub(super) snapshot: LiveWorkSnapshot,
}

impl LiveWorkSource for LiveWorkFixture {
    fn live_work(&self) -> Result<LiveWorkSnapshot> {
        Ok(self.snapshot.clone())
    }
}

pub(super) fn authenticated_profile(
    engine: Engine,
    account: &str,
    path: PathBuf,
) -> ProfileSnapshot {
    ProfileSnapshot {
        engine,
        account: account.into(),
        path,
        reserved: account == "orch",
        auth: AuthStatus::Authenticated {
            methods: vec![AuthMethod::ApiKey],
        },
        eligibility: ProfileEligibility {
            credential_present: true,
            authenticated: true,
            worker_eligible: account != "orch",
            orchestrator_eligible: true,
            rotation_eligible: false,
            managed_pool_eligible: true,
        },
    }
}

pub(super) fn routing_runtime(
    root: &std::path::Path,
    claude_binary: bool,
    kimi_binary: bool,
) -> ProviderRuntime {
    let claude_profile = authenticated_profile(
        Engine::Claude,
        "orch",
        root.join("custom/claude-orchestrator"),
    );
    let kimi_profile = authenticated_profile(Engine::Kimi, "1", root.join("custom/kimi-account"));
    let mut providers = Engine::ALL
        .into_iter()
        .map(|engine| {
            (
                engine,
                ProviderSnapshot {
                    spec: spec(engine),
                    binary: BinaryStatus {
                        program: format!("{engine}-fixture"),
                        available: false,
                        version: None,
                    },
                    profiles: Vec::new(),
                    models: vec![spec(engine).default_model],
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    providers.insert(
        Engine::Claude,
        ProviderSnapshot {
            spec: spec(Engine::Claude),
            binary: BinaryStatus {
                program: "claude-fixture".into(),
                available: claude_binary,
                version: None,
            },
            profiles: vec![claude_profile],
            models: vec![spec(Engine::Claude).default_model],
        },
    );
    providers.insert(
        Engine::Kimi,
        ProviderSnapshot {
            spec: spec(Engine::Kimi),
            binary: BinaryStatus {
                program: "kimi-fixture".into(),
                available: kimi_binary,
                version: None,
            },
            profiles: vec![kimi_profile],
            models: vec![spec(Engine::Kimi).default_model],
        },
    );
    ProviderRuntime::from_catalog(CatalogSnapshot { providers })
}
