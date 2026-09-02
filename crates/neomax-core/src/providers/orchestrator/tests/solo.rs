use std::ffi::OsStr;

use super::support::request;
use crate::Engine;
use crate::providers::{build_orchestrator_command, worker::ORCHESTRATOR_DIRECTIVE};

#[test]
fn solo_commands_are_plain_provider_sessions_without_orchestrator_injection() {
    for engine in Engine::ALL {
        let request = request(engine).with_solo(true);
        let command =
            build_orchestrator_command(engine, OsStr::new("fixture-provider"), &request).unwrap();
        let args = command.args_lossy();
        assert!(!args.iter().any(|arg| arg == ORCHESTRATOR_DIRECTIVE));
        assert!(!command.env.contains_key(OsStr::new("NEOMAX_ROLE")));
        assert!(!command.env.contains_key(OsStr::new("NEOMAX_ORCHESTRATOR")));
        assert_eq!(
            command.env.get(OsStr::new("NEOMAX_MODE")),
            Some(&"solo".into())
        );
        if engine == Engine::Opencode {
            assert_eq!(
                command.env.get(OsStr::new("OPENCODE_AUTO_SHARE")),
                Some(&"false".into())
            );
        }
        assert!(
            command
                .env_remove
                .iter()
                .any(|key| key == OsStr::new("NEOMAX_BIN"))
        );
        assert!(
            command
                .env_remove
                .iter()
                .any(|key| key == OsStr::new("NEOMAX_ROLE"))
        );
        assert!(
            command
                .env_remove
                .iter()
                .any(|key| key == OsStr::new("NEOMAX_ORCHESTRATOR_ORIENTATION"))
        );
        assert!(!args.contains(&"exec".into()));
        assert!(!args.contains(&"run".into()));
        assert!(!args.contains(&"--prompt".into()));
    }
}

#[test]
fn solo_claude_keeps_ultracode_defaults_and_accepts_a_plain_initial_task() {
    let request = request(Engine::Claude)
        .with_solo(true)
        .with_initial_task("fixture solo task");
    let command =
        build_orchestrator_command(Engine::Claude, OsStr::new("claude"), &request).unwrap();
    assert_eq!(
        command.args_lossy(),
        vec![
            "--model",
            crate::providers::catalog::CLAUDE_DEFAULT_MODEL,
            "--dangerously-skip-permissions",
            "--settings",
            r#"{"ultracode":true}"#,
            "--effort",
            "xhigh",
            "fixture solo task",
        ]
    );
}
