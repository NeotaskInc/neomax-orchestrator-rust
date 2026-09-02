use std::ffi::OsStr;
use std::path::PathBuf;

use crate::agent_tools::{
    MANIFEST_RELATIVE_PATH, ManifestStore, NEOMAX_BIN_ENV, NEOMAX_TOOL_DEPTH_ENV,
    NEOMAX_TOOL_INSTRUCTION_ENV, NEOMAX_TOOL_MANIFEST_ENV, NEOMAX_TOOL_MAX_DEPTH_ENV,
    NEOMAX_TOOL_POLICY_ENV, ToolManifest,
};
use crate::providers::{Kimi, Provider, ProviderRegistry};
use crate::runs::RunRecord;
use crate::settings::{
    LANES_PER_ACCOUNT_ENV, MAX_LIVE_ENV, MAX_SESSIONS_PER_ACCOUNT_ENV, MAX_SUBAGENTS_ENV,
    MAX_TASKS_ENV, QUEUE_TTL_SECONDS_ENV,
};
use crate::{Engine, StatePaths};

use super::super::super::prepare_attempt;
use super::support::settings;

#[test]
fn keeps_kimi_plan_home_alive_and_exports_agent_limits() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    std::fs::create_dir_all(&profile).unwrap();
    let run: RunRecord = serde_json::from_value(serde_json::json!({
        "id":"run", "engine":"kimi", "model":"kimi-code/k3", "prompt":"inspect",
        "profile":profile, "workdir":temp.path(), "status":"running", "started":1,
        "plan_mode":true
    }))
    .unwrap();
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    let provider = Kimi::new("kimi");
    let prepared = prepare_attempt(&provider, &run, &settings(), &paths, None).unwrap();
    let home = prepared
        .command()
        .env
        .get(std::ffi::OsStr::new("KIMI_CODE_HOME"))
        .map(std::path::PathBuf::from)
        .unwrap();
    assert!(home.exists());
    assert_eq!(
        prepared
            .command()
            .env
            .get(std::ffi::OsStr::new("NEOMAX_MAX_SUBAGENTS")),
        Some(&"77".into())
    );
    assert_eq!(provider.engine(), Engine::Kimi);
    drop(prepared);
    assert!(!home.exists());
}

#[test]
fn every_production_provider_launch_receives_the_prepared_tool_contract() {
    let temp = tempfile::tempdir().unwrap();
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    let settings = settings();
    let providers = ProviderRegistry::standard();
    let manifest_path = paths.state.join(MANIFEST_RELATIVE_PATH);

    for engine in Engine::ALL {
        let profile = temp.path().join(format!("profile-{engine}"));
        let run: RunRecord = serde_json::from_value(serde_json::json!({
            "id": format!("run-{engine}"),
            "engine": engine,
            "model": format!("{engine}/worker-model"),
            "profile": profile,
            "workdir": temp.path(),
            "status": "running",
            "started": 1,
            "environment": {
                "NEOMAX_ROLE": engine.as_str(),
                "NEOMAX_FLEET": "claude,codex,opencode,kimi,grok"
            }
        }))
        .unwrap();
        let prepared = prepare_attempt(
            providers.get(engine).unwrap(),
            &run,
            &settings,
            &paths,
            None,
        )
        .unwrap();
        let env = &prepared.command().env;
        for (key, value) in prepared._launch_context.tools().variables() {
            let expected = std::ffi::OsString::from(value);
            assert_eq!(env.get(OsStr::new(key)), Some(&expected));
        }
        let expected_manifest = std::fs::canonicalize(&manifest_path).unwrap();
        assert!(env.contains_key(OsStr::new(NEOMAX_BIN_ENV)));
        assert!(env.contains_key(OsStr::new(NEOMAX_TOOL_DEPTH_ENV)));
        assert!(env.contains_key(OsStr::new(NEOMAX_TOOL_MAX_DEPTH_ENV)));
        let actual_manifest = env
            .get(OsStr::new(NEOMAX_TOOL_MANIFEST_ENV))
            .map(PathBuf::from)
            .unwrap();
        assert_eq!(
            std::fs::canonicalize(actual_manifest).unwrap(),
            expected_manifest
        );
        assert_eq!(
            env.get(OsStr::new(NEOMAX_TOOL_POLICY_ENV)),
            Some(&"worker".into())
        );
        assert_eq!(env.get(OsStr::new("NEOMAX_WORKER")), Some(&"1".into()));
        assert_eq!(env.get(OsStr::new("NEOMAX_ORCHESTRATOR")), None);
        assert_eq!(
            env.get(OsStr::new("NEOMAX_ROLE")),
            Some(&engine.to_string().into())
        );
        assert_eq!(
            env.get(OsStr::new("NEOMAX_FLEET")),
            Some(&"claude,codex,opencode,kimi,grok".into())
        );
        for key in [
            MAX_SUBAGENTS_ENV,
            MAX_TASKS_ENV,
            MAX_SESSIONS_PER_ACCOUNT_ENV,
            LANES_PER_ACCOUNT_ENV,
            QUEUE_TTL_SECONDS_ENV,
            MAX_LIVE_ENV,
        ] {
            assert!(
                env.contains_key(OsStr::new(key)),
                "{engine} launch is missing shared setting {key}"
            );
        }
        assert!(
            env.get(OsStr::new(NEOMAX_BIN_ENV))
                .is_some_and(|value| std::path::Path::new(value).is_file())
        );
        assert!(
            env.get(OsStr::new(NEOMAX_TOOL_INSTRUCTION_ENV))
                .is_some_and(|value| value.to_string_lossy().contains("NEOMAX_BIN"))
        );
        let manifest = ManifestStore::new(std::path::PathBuf::from(
            env.get(OsStr::new(NEOMAX_TOOL_MANIFEST_ENV)).unwrap(),
        ))
        .read_private_canonical()
        .unwrap();
        assert_eq!(manifest, ToolManifest::canonical());
        assert!(
            prepared
                .command()
                .args_lossy()
                .contains(&format!("{engine}/worker-model"))
        );
    }
}
