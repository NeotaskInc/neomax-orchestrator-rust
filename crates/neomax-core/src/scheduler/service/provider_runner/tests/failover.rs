use std::collections::BTreeMap;
use std::sync::Arc;

use super::super::{ProviderExecution, ProviderExecutionConfig};
use super::support::FixtureProvider;
use crate::providers::{Provider, ProviderProfile, ProviderRegistry};
#[cfg(unix)]
use crate::runs::RunStore;
use crate::scheduler::runtime::DispatchRequest;
#[cfg(unix)]
use crate::scheduler::runtime::WorkerOutcome;
use crate::{EffectiveSettings, Engine, SettingsFile, StatePaths};
#[cfg(unix)]
use crate::WorkerScope;

#[test]
fn scheduler_attempt_number_does_not_become_provider_failover_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ProviderProfile {
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
        attempt: 7,
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
    let run = execution.new_run(&request).unwrap();
    assert_eq!(run.attempt, 1);
    assert_eq!(run.tag.as_deref(), Some("plan"));
    assert_eq!(
        run.extra.get("scheduler_attempt"),
        Some(&serde_json::json!(7))
    );
}

#[cfg(unix)]
#[test]
fn scheduler_composition_passes_configured_target_model_through_failover() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let source_script = temp.path().join("source.sh");
    std::fs::write(
        &source_script,
        "#!/bin/sh\nprintf '%s\\n' 'rate limit' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&source_script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let target_script = temp.path().join("target.sh");
    std::fs::write(
        &target_script,
        "#!/bin/sh\nprintf '%s' \"$1\" > \"$MODEL_CAPTURE\"\nprintf '%s\\n' '{\"type\":\"result\",\"subtype\":\"success\"}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&target_script, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(
        temp.path().join("models.toml"),
        "opencode = 'local/custom/model:latest'\n",
    )
    .unwrap();

    let source_profile = ProviderProfile {
        engine: Engine::Claude,
        account: "source".into(),
        path: temp.path().join("claude-profile"),
        reserved: false,
    };
    let target_profile = ProviderProfile {
        engine: Engine::Opencode,
        account: "target".into(),
        path: temp.path().join("opencode-profile"),
        reserved: false,
    };
    let capture = temp.path().join("target-model.txt");
    let providers = Arc::new(ProviderRegistry::new([
        Box::new(super::support::ScriptProvider {
            engine: Engine::Claude,
            profile: source_profile,
            executable: source_script,
            default_model: "claude-fixture-model".into(),
            model_capture: None,
        }) as Box<dyn Provider>,
        Box::new(super::support::ScriptProvider {
            engine: Engine::Opencode,
            profile: target_profile,
            executable: target_script,
            default_model: "opencode-fixture-model".into(),
            model_capture: Some(capture.clone()),
        }),
    ]));
    let settings = Arc::new(
        EffectiveSettings::resolve(
            SettingsFile::default(),
            temp.path().join("config.toml"),
            &BTreeMap::new(),
        )
        .unwrap(),
    );
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    let execution = ProviderExecution::new(
        ProviderExecutionConfig::new(providers, settings, paths.clone())
            .with_scope(WorkerScope::all()),
    )
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
    let run = execution.new_run(&request).unwrap();
    let internal_run_id = run.id.clone();
    RunStore::new(paths.runs.clone()).create(&run).unwrap();
    let outcome = execution.execute_run("plan-part", run).unwrap();
    assert!(matches!(outcome, WorkerOutcome::Completed { .. }));
    let saved = RunStore::new(paths.runs).load(&internal_run_id).unwrap();
    assert_eq!(saved.engine, Engine::Opencode);
    assert_eq!(saved.model, "local/custom/model:latest");
    assert_eq!(saved.attempt, 2);
    assert_eq!(
        std::fs::read_to_string(capture).unwrap(),
        "local/custom/model:latest"
    );
}
