use std::ffi::OsStr;

use crate::agent_tools::LaunchRole;
use crate::providers::ProviderRegistry;
use crate::runs::RunRecord;
use crate::{Engine, StatePaths};

use super::super::super::prepare_attempt;
use super::support::{argument_value, orchestrator_profile, settings};

#[test]
fn kimi_orchestrator_bootstraps_the_task_then_keeps_the_interactive_prompt_clean() {
    let temp = tempfile::tempdir().unwrap();
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    let settings = settings();
    let providers = ProviderRegistry::standard();
    let profile = orchestrator_profile(temp.path(), "kimi-handoff-profile", Engine::Kimi);
    let mut run: RunRecord = serde_json::from_value(serde_json::json!({
        "id": "kimi-handoff-run",
        "engine": "kimi",
        "model": "kimi-code/k3",
        "prompt": "continue the durable task",
        "profile": profile,
        "workdir": temp.path(),
        "status": "running",
        "started": 1,
        "launch_role": "orchestrator"
    }))
    .unwrap();
    run.launch_role = LaunchRole::Orchestrator;

    let prepared = prepare_attempt(
        providers.get(Engine::Kimi).unwrap(),
        &run,
        &settings,
        &paths,
        None,
    )
    .unwrap();
    let args = prepared.command().args_lossy();
    assert!(args.iter().any(|arg| arg == "--agent-file"));
    assert!(!args.iter().any(|arg| arg == "--prompt"));
    assert!(!args.iter().any(|arg| arg == "continue the durable task"));
    let bootstrap = prepared
        .bootstrap_command()
        .expect("Kimi initial task needs a headless bootstrap");
    let bootstrap_args = bootstrap.args_lossy();
    assert!(bootstrap_args.iter().any(|arg| arg == "--prompt"));
    assert!(
        bootstrap_args
            .iter()
            .any(|arg| arg.contains("continue the durable task"))
    );
    assert_eq!(run.prompt_for_attempt(), "continue the durable task");
}

#[test]
fn root_effort_and_ultra_are_carried_into_typed_orchestrator_requests() {
    let temp = tempfile::tempdir().unwrap();
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    let settings = settings();
    let providers = ProviderRegistry::standard();
    let cases = [
        (Engine::Claude, Some("max"), true),
        (Engine::Codex, Some("medium"), false),
        (Engine::Codex, None, true),
    ];

    for (index, (engine, effort, ultra)) in cases.into_iter().enumerate() {
        let profile = orchestrator_profile(
            temp.path(),
            &format!("root-profile-{engine}-{index}"),
            engine,
        );
        let mut run: RunRecord = serde_json::from_value(serde_json::json!({
            "id": format!("root-settings-{engine}-{index}"),
            "engine": engine,
            "model": format!("{engine}/root-model"),
            "profile": profile,
            "workdir": temp.path(),
            "status": "running",
            "started": 1,
            "launch_role": "orchestrator"
        }))
        .unwrap();
        run.launch_role = LaunchRole::Orchestrator;
        run.effort = effort.map(str::to_owned);
        run.ultra = ultra;

        let prepared = prepare_attempt(
            providers.get(engine).unwrap(),
            &run,
            &settings,
            &paths,
            None,
        )
        .unwrap();
        let args = prepared.command().args_lossy();
        match engine {
            Engine::Claude => {
                assert_eq!(argument_value(&args, "--effort"), Some("max"));
                assert_eq!(
                    argument_value(&args, "--settings"),
                    Some(r#"{"ultracode":true}"#)
                );
            }
            Engine::Codex if effort.is_some() => {
                assert!(args.contains(&"model_reasoning_effort=medium".into()));
            }
            Engine::Codex => {
                assert!(args.contains(&"model_reasoning_effort=xhigh".into()));
            }
            _ => unreachable!("root settings test only covers supported providers"),
        }
    }
}

#[test]
fn root_goal_and_turn_limits_are_carried_into_typed_orchestrator_requests() {
    let temp = tempfile::tempdir().unwrap();
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    let settings = settings();
    let providers = ProviderRegistry::standard();

    for engine in [
        Engine::Claude,
        Engine::Codex,
        Engine::Opencode,
        Engine::Grok,
    ] {
        let profile =
            orchestrator_profile(temp.path(), &format!("root-goal-profile-{engine}"), engine);
        let mut run: RunRecord = serde_json::from_value(serde_json::json!({
            "id": format!("root-goal-{engine}"),
            "engine": engine,
            "model": format!("{engine}/root-model"),
            "profile": profile,
            "workdir": temp.path(),
            "status": "running",
            "started": 1,
            "goal": "the root objective passes",
            "max_turns": 3,
            "launch_role": "orchestrator"
        }))
        .unwrap();
        run.launch_role = LaunchRole::Orchestrator;
        run.supervisor_pid = Some(4242);

        let prepared = prepare_attempt(
            providers.get(engine).unwrap(),
            &run,
            &settings,
            &paths,
            None,
        )
        .unwrap();
        assert_eq!(
            prepared.command().env.get(OsStr::new("NEOMAX_ORCH_PID")),
            Some(&"4242".into())
        );
        let args = prepared.command().args_lossy();
        match engine {
            Engine::Claude => {
                assert!(args.contains(&"--max-turns".into()));
                assert!(args.iter().any(|arg| arg.starts_with("/goal ")));
            }
            Engine::Codex | Engine::Opencode | Engine::Grok => {
                assert!(
                    args.iter()
                        .any(|arg| arg.contains("OBJECTIVE: do not finish"))
                );
                assert!(args.iter().any(|arg| arg.contains("Make at most 3")));
            }
            Engine::Kimi => unreachable!(),
        }
    }
}
