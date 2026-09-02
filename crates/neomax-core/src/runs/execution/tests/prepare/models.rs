use std::ffi::OsStr;

use crate::agent_tools::LaunchRole;
use crate::providers::ProviderRegistry;
use crate::runs::RunRecord;
use crate::{Engine, StatePaths};

use super::super::super::prepare_attempt;
use super::support::{orchestrator_profile, settings};

#[test]
fn orchestrator_propagates_each_worker_model_to_every_provider_environment() {
    let temp = tempfile::tempdir().unwrap();
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    let settings = settings();
    let providers = ProviderRegistry::standard();
    let models = serde_json::json!({
        "claude": "claude/local-worker",
        "codex": "gpt-5.6-terra",
        "opencode": "local/provider/model",
        "kimi": "kimi-code/kimi-for-coding",
        "grok": "grok/local-worker"
    });
    let expected = [
        ("NEOMAX_CLAUDE_MODEL", "claude/local-worker"),
        ("NEOMAX_CODEX_MODEL", "gpt-5.6-terra"),
        ("NEOMAX_OPENCODE_MODEL", "local/provider/model"),
        ("NEOMAX_KIMI_MODEL", "kimi-code/kimi-for-coding"),
        ("NEOMAX_GROK_MODEL", "grok/local-worker"),
    ];

    for engine in Engine::ALL {
        let profile = orchestrator_profile(temp.path(), &format!("profile-{engine}"), engine);
        let mut run: RunRecord = serde_json::from_value(serde_json::json!({
            "id": format!("model-run-{engine}"),
            "engine": engine,
            "model": format!("{engine}/root-model"),
            "profile": profile,
            "workdir": temp.path(),
            "status": "running",
            "started": 1,
            "launch_role": "orchestrator",
            "worker_scope": "claude,opencode",
            "worker_models": models
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
        let root_model_key = match engine {
            Engine::Claude => "NEOMAX_CLAUDE_MODEL",
            Engine::Codex => "NEOMAX_CODEX_MODEL",
            Engine::Opencode => "NEOMAX_OPENCODE_MODEL",
            Engine::Kimi => "NEOMAX_KIMI_MODEL",
            Engine::Grok => "NEOMAX_GROK_MODEL",
        };
        for (key, value) in expected {
            let expected_value = if key == root_model_key {
                format!("{engine}/root-model")
            } else {
                value.to_string()
            };
            assert_eq!(
                prepared.command().env.get(OsStr::new(key)),
                Some(&expected_value.into()),
                "missing {key} for {engine} orchestrator"
            );
        }
        assert_eq!(
            prepared.command().env.get(OsStr::new("NEOMAX_FLEET")),
            Some(&"claude,opencode".into())
        );
        assert_eq!(
            prepared.command().env.get(OsStr::new("NEOMAX_WORKERS")),
            Some(&"claude,opencode".into())
        );
    }
}
