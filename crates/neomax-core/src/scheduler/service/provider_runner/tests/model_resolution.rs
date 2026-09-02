use std::collections::BTreeMap;
use std::sync::Arc;

use super::super::{resolve_scheduler_model, ProviderExecution, ProviderExecutionConfig};
use super::support::FixtureProvider;
use crate::providers::{Provider, ProviderRegistry};
use crate::scheduler::runtime::DispatchRequest;
use crate::{EffectiveSettings, Engine, SettingsFile, StatePaths};

#[test]
fn scheduler_initial_model_uses_explicit_config_environment_then_default() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.toml");
    let environment = BTreeMap::from([("NEOMAX_CLAUDE_MODEL".into(), "claude/environment".into())]);
    std::fs::write(
        temp.path().join("models.toml"),
        "claude = 'claude/config'\n",
    )
    .unwrap();

    assert_eq!(
        resolve_scheduler_model(&config_path, Engine::Claude, None, &environment).unwrap(),
        "claude/config"
    );
    assert_eq!(
        resolve_scheduler_model(
            &config_path,
            Engine::Claude,
            Some("claude/explicit"),
            &environment,
        )
        .unwrap(),
        "claude/explicit"
    );

    std::fs::remove_file(temp.path().join("models.toml")).unwrap();
    assert_eq!(
        resolve_scheduler_model(&config_path, Engine::Claude, None, &environment).unwrap(),
        "claude/environment"
    );
    assert_eq!(
        resolve_scheduler_model(&config_path, Engine::Claude, None, &BTreeMap::new(),).unwrap(),
        "claude-fable-5[1m]"
    );
}

#[test]
fn provider_runner_keeps_invalid_models_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let profile = crate::providers::ProviderProfile {
        engine: Engine::Claude,
        account: "fixture".into(),
        path: temp.path().join("profile"),
        reserved: false,
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
        model: Some("invalid model".into()),
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
        crate::scheduler::runtime::DispatchError::Terminal { .. }
    ));
}
