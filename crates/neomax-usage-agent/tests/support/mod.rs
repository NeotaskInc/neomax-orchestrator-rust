use neomax_core::config::{Engine, StatePaths};
use neomax_core::providers::catalog::{
    AuthStatus, BinaryStatus, CatalogSnapshot, ProfileEligibility, ProfileSnapshot,
    ProviderSnapshot, spec,
};
use neomax_usage_agent::AgentPaths;

pub(crate) fn agent_paths(temp: &tempfile::TempDir) -> AgentPaths {
    let home = temp.path();
    let providers = Engine::ALL
        .into_iter()
        .map(|engine| {
            let provider = spec(engine);
            (
                engine,
                ProviderSnapshot {
                    spec: provider.clone(),
                    binary: BinaryStatus {
                        program: provider.default_binary.clone(),
                        available: false,
                        version: None,
                    },
                    profiles: vec![ProfileSnapshot {
                        engine,
                        account: "default".into(),
                        path: home.join(&provider.default_profile_dir),
                        reserved: false,
                        auth: AuthStatus::Unauthenticated,
                        eligibility: ProfileEligibility::disconnected(),
                    }],
                    models: Vec::new(),
                },
            )
        })
        .collect();
    AgentPaths::for_state(StatePaths::new(home, home.join(".neomax")))
        .with_catalog(CatalogSnapshot { providers })
}
