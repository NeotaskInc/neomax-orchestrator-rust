use std::ffi::OsStr;
use std::path::PathBuf;

use crate::agent_tools::{
    LaunchRole, MANIFEST_RELATIVE_PATH, ManifestStore, NEOMAX_BIN_ENV, NEOMAX_TOOL_DEPTH_ENV,
    NEOMAX_TOOL_INSTRUCTION_ENV, NEOMAX_TOOL_MANIFEST_ENV, NEOMAX_TOOL_POLICY_ENV, ToolManifest,
    ToolPolicy,
};
use crate::providers::{ORCHESTRATOR_DIRECTIVE, ProviderRegistry};
use crate::runs::RunRecord;
use crate::{Engine, StatePaths};

use super::super::super::prepare_attempt;
use super::support::{orchestrator_profile, settings};

#[test]
fn every_orchestrator_provider_launch_receives_full_tools_and_role_contract() {
    let temp = tempfile::tempdir().unwrap();
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    let settings = settings();
    let providers = ProviderRegistry::standard();
    let manifest_path = paths.state.join(MANIFEST_RELATIVE_PATH);
    let manifest = ToolManifest::canonical();
    manifest.validate().unwrap();
    assert!(
        ToolPolicy::orchestrator()
            .authorize(&manifest, "dispatch")
            .is_ok()
    );

    for engine in Engine::ALL {
        let profile = orchestrator_profile(
            temp.path(),
            &format!("orchestrator-profile-{engine}"),
            engine,
        );
        let mut run: RunRecord = serde_json::from_value(serde_json::json!({
            "id": format!("orchestrator-run-{engine}"),
            "engine": engine,
            "profile": profile,
            "workdir": temp.path(),
            "status": "running",
            "started": 1,
            "launch_role": "orchestrator",
            "environment": {"NEOMAX_ROLE": engine.as_str()}
        }))
        .unwrap();
        run.launch_role = LaunchRole::Orchestrator;
        let prepared = prepare_attempt(
            providers.get(engine).unwrap(),
            &run,
            &settings,
            &paths,
            None,
        )
        .unwrap();
        let context = &prepared._launch_context;
        assert_eq!(context.role(), LaunchRole::Orchestrator);
        let env = &prepared.command().env;
        for (key, value) in context.tools().variables() {
            assert_eq!(
                env.get(OsStr::new(key)),
                Some(&value.into()),
                "missing prepared tool variable {key} for {engine} orchestrator"
            );
        }
        assert_eq!(
            env.get(OsStr::new(NEOMAX_TOOL_POLICY_ENV)),
            Some(&"orchestrator".into())
        );
        assert_eq!(env.get(OsStr::new("NEOMAX_WORKER")), None);
        assert_eq!(
            env.get(OsStr::new("NEOMAX_ORCHESTRATOR")),
            Some(&"1".into())
        );
        assert_eq!(
            env.get(OsStr::new("NEOMAX_ROLE")),
            Some(&engine.to_string().into())
        );
        assert_eq!(
            env.get(OsStr::new(NEOMAX_TOOL_DEPTH_ENV)),
            Some(&"0".into())
        );
        let actual_manifest = PathBuf::from(env.get(OsStr::new(NEOMAX_TOOL_MANIFEST_ENV)).unwrap());
        assert_eq!(
            std::fs::canonicalize(actual_manifest).unwrap(),
            std::fs::canonicalize(&manifest_path).unwrap()
        );
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
        assert_eq!(
            env.get(OsStr::new("NEOMAX_FLEET")),
            Some(&"claude,codex,opencode,kimi,grok".into())
        );
        assert_eq!(
            env.get(OsStr::new("NEOMAX_WORKERS")),
            Some(&"claude,codex,opencode,kimi,grok".into())
        );
        let args = prepared.command().args_lossy();
        if engine == Engine::Kimi {
            assert!(args.iter().any(|arg| arg == "--agent-file"));
        } else {
            assert!(args.iter().any(|arg| arg.contains(ORCHESTRATOR_DIRECTIVE)));
        }
    }
}
