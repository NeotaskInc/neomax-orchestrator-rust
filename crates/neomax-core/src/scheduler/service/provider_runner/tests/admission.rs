use std::collections::BTreeMap;
use std::sync::Arc;

use super::super::{ProviderExecution, ProviderExecutionConfig};
use super::support::FixtureProvider;
use crate::providers::catalog::{
    spec, AuthMethod, AuthStatus, BinaryStatus, CatalogSnapshot, ProfileEligibility,
    ProfileSnapshot, ProviderSnapshot,
};
use crate::providers::{Provider, ProviderProfile, ProviderRegistry};
use crate::scheduler::runtime::DispatchRequest;
use crate::{EffectiveSettings, Engine, SettingsFile, StatePaths};

#[test]
fn provider_runner_defers_when_no_account_is_currently_eligible() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ProviderProfile {
        engine: Engine::Claude,
        account: "paused-fixture".into(),
        path: temp.path().join("profile"),
        reserved: true,
    };
    let providers = Arc::new(ProviderRegistry::new([
        Box::new(FixtureProvider { profile }) as Box<dyn Provider>,
    ]));
    let settings = Arc::new(
        EffectiveSettings::resolve(
            SettingsFile::default(),
            temp.path().join("config.toml"),
            &BTreeMap::new(),
        )
        .unwrap(),
    );
    let execution = ProviderExecution::new(ProviderExecutionConfig::new(
        providers,
        settings,
        StatePaths::new(temp.path(), temp.path().join("state")),
    ))
    .unwrap();
    let request = DispatchRequest {
        plan_id: "plan".into(),
        part_id: "part".into(),
        run_id: "plan-part".into(),
        attempt: 1,
        engine: Engine::Claude,
        model: None,
        prompt: "work".into(),
        areas: Vec::new(),
        dependencies: Vec::new(),
        cwd: temp.path().to_path_buf(),
        repository: None,
        branch: None,
        base: None,
        environment: BTreeMap::new(),
    };
    let error = execution.new_run_classified(&request).unwrap_err();
    assert!(matches!(
        error,
        crate::scheduler::runtime::DispatchError::Deferred { .. }
    ));
}

#[test]
fn provider_runner_defers_missing_binary_but_keeps_invalid_models_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ProviderProfile {
        engine: Engine::Claude,
        account: "missing-binary".into(),
        path: temp.path().join("profile"),
        reserved: false,
    };
    let providers = ProviderRegistry::new([Box::new(FixtureProvider {
        profile: profile.clone(),
    }) as Box<dyn Provider>])
    .with_catalog(CatalogSnapshot {
        providers: BTreeMap::from([(
            Engine::Claude,
            ProviderSnapshot {
                spec: spec(Engine::Claude),
                binary: BinaryStatus {
                    program: "fixture-provider".into(),
                    available: false,
                    version: None,
                },
                profiles: vec![ProfileSnapshot {
                    engine: Engine::Claude,
                    account: profile.account.clone(),
                    path: profile.path.clone(),
                    reserved: profile.reserved,
                    auth: AuthStatus::Authenticated {
                        methods: vec![AuthMethod::OAuth],
                    },
                    eligibility: ProfileEligibility {
                        credential_present: true,
                        authenticated: true,
                        worker_eligible: true,
                        orchestrator_eligible: true,
                        rotation_eligible: true,
                        managed_pool_eligible: true,
                    },
                }],
                models: vec![spec(Engine::Claude).default_model],
            },
        )]),
    });
    let settings = Arc::new(
        EffectiveSettings::resolve(
            SettingsFile::default(),
            temp.path().join("config.toml"),
            &BTreeMap::new(),
        )
        .unwrap(),
    );
    let execution = ProviderExecution::new(ProviderExecutionConfig::new(
        Arc::new(providers),
        settings,
        StatePaths::new(temp.path(), temp.path().join("state")),
    ))
    .unwrap();
    let request = DispatchRequest {
        plan_id: "plan".into(),
        part_id: "part".into(),
        run_id: "plan-part".into(),
        attempt: 1,
        engine: Engine::Claude,
        model: None,
        prompt: "work".into(),
        areas: Vec::new(),
        dependencies: Vec::new(),
        cwd: temp.path().to_path_buf(),
        repository: None,
        branch: None,
        base: None,
        environment: BTreeMap::new(),
    };
    let error = execution.new_run_classified(&request).unwrap_err();
    assert!(matches!(
        error,
        crate::scheduler::runtime::DispatchError::Deferred { .. }
    ));

    let mut invalid_model = request;
    invalid_model.model = Some("invalid model".into());
    let error = execution.new_run_classified(&invalid_model).unwrap_err();
    assert!(matches!(
        error,
        crate::scheduler::runtime::DispatchError::Terminal { .. }
    ));
}
