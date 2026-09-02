use std::ffi::OsStr;

use super::support::{args, request};
use crate::Engine;
use crate::providers::catalog;

#[test]
fn no_task_uses_the_installed_agent_file_contract() {
    let expected_agent_file = std::path::PathBuf::from(super::support::HOME)
        .join("kimi-agent.md")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        args(Engine::Kimi, &request(Engine::Kimi)),
        vec![
            String::from("-m"),
            catalog::KIMI_DEFAULT_MODEL.into(),
            String::from("--auto"),
            String::from("--agent-file"),
            expected_agent_file,
        ]
    );
}

#[test]
fn goals_and_max_turns_are_rejected_for_interactive_kimi() {
    let request = request(Engine::Kimi)
        .with_goal("the tests pass")
        .with_max_turns(3);
    let error = super::super::build(Engine::Kimi, OsStr::new("kimi"), &request).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("do not support --goal or --max-turns")
    );
}

#[test]
fn resume_keeps_the_bound_agent_without_a_prompt_or_agent_rebind() {
    let request = request(Engine::Kimi)
        .with_session("session-42")
        .with_resume(true);
    let command = super::super::build(Engine::Kimi, OsStr::new("kimi"), &request).unwrap();
    assert_eq!(
        command.args_lossy(),
        vec![
            "-m",
            catalog::KIMI_DEFAULT_MODEL,
            "--auto",
            "-S",
            "session-42"
        ]
    );
    assert!(!command.args_lossy().contains(&"--agent-file".into()));
    assert!(!command.args_lossy().contains(&"-p".into()));
    assert!(!command.args_lossy().contains(&"--prompt".into()));
}

#[test]
fn default_agent_path_is_derived_from_the_profile() {
    let mut request = request(Engine::Kimi);
    request.agent_file = None;
    let command = super::super::build(Engine::Kimi, OsStr::new("kimi"), &request).unwrap();
    let expected_agent_file = std::path::PathBuf::from(super::support::HOME)
        .join(".kimi-code")
        .join("agents")
        .join("neomax.md")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        command.args_lossy().last().map(String::as_str),
        Some(expected_agent_file.as_str())
    );
}

#[test]
fn initial_tasks_use_a_headless_bootstrap_before_the_interactive_session() {
    let request = request(Engine::Kimi).with_initial_task("fixture task");
    let command = super::super::build(Engine::Kimi, OsStr::new("kimi"), &request).unwrap();
    assert!(command.args_lossy().contains(&"--agent-file".into()));
    assert!(!command.args_lossy().contains(&"--prompt".into()));

    let bootstrap = super::super::build_bootstrap(Engine::Kimi, OsStr::new("kimi"), &request)
        .unwrap()
        .expect("initial Kimi tasks need a bootstrap");
    let args = bootstrap.args_lossy();
    assert_eq!(args.first().map(String::as_str), Some("-m"));
    assert_eq!(
        args.get(1).map(String::as_str),
        Some(catalog::KIMI_DEFAULT_MODEL)
    );
    assert_eq!(args.get(2).map(String::as_str), Some("--output-format"));
    assert_eq!(args.get(3).map(String::as_str), Some("stream-json"));
    assert_eq!(args.get(4).map(String::as_str), Some("--prompt"));
    assert!(
        args.last()
            .is_some_and(|prompt| prompt.contains("fixture task"))
    );
    assert!(!args.contains(&"--auto".into()));
}

#[test]
fn solo_initial_tasks_use_a_plain_bootstrap_without_orchestrator_environment() {
    let request = request(Engine::Kimi)
        .with_solo(true)
        .with_initial_task("solo fixture task");
    let bootstrap = super::super::build_bootstrap(Engine::Kimi, OsStr::new("kimi"), &request)
        .unwrap()
        .expect("solo Kimi initial tasks need a bootstrap");
    let args = bootstrap.args_lossy();
    assert!(args.contains(&"--prompt".into()));
    assert!(args.iter().any(|arg| arg == "solo fixture task"));
    assert!(!bootstrap.env.contains_key(OsStr::new("NEOMAX_ORCHESTRATOR")));
    assert!(!bootstrap.env.contains_key(OsStr::new("NEOMAX_TOOL_MANIFEST")));
}
