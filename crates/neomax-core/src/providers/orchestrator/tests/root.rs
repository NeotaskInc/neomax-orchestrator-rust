use std::ffi::OsStr;

use super::support::request;
use crate::Engine;
use crate::providers::worker::ORCHESTRATOR_DIRECTIVE;

#[test]
fn task_and_resume_keep_provider_interactive_shapes() {
    for engine in Engine::ALL {
        if engine == Engine::Kimi {
            continue;
        }
        let request = request(engine)
            .with_model("local/provider/model:with-arbitrary.punctuation_v2")
            .with_initial_task("fixture task")
            .with_session("session-42")
            .with_resume(true);
        let command =
            super::super::build(engine, OsStr::new("fixture-provider"), &request).unwrap();
        let args = command.args_lossy();
        assert!(
            args.iter().any(|arg| arg.contains("fixture task")),
            "{engine}: {args:?}"
        );
        assert!(args.contains(&"session-42".into()), "{engine}: {args:?}");
        assert!(!args.contains(&"-p".into()), "{engine}: {args:?}");
        assert!(!args.contains(&"exec".into()), "{engine}: {args:?}");
        assert!(!args.contains(&"run".into()), "{engine}: {args:?}");
        assert!(!args.contains(&"--single".into()), "{engine}: {args:?}");
        assert!(args.contains(&"local/provider/model:with-arbitrary.punctuation_v2".into()));
    }
}

#[test]
fn no_task_orientation_reaches_each_supported_provider_surface() {
    let orientation = "fixture orientation";
    for engine in [
        Engine::Claude,
        Engine::Codex,
        Engine::Opencode,
        Engine::Grok,
    ] {
        let request = request(engine).with_orientation(orientation);
        let command = super::super::build(engine, OsStr::new(engine.as_str()), &request).unwrap();
        let args = command.args_lossy();
        match engine {
            Engine::Claude => {
                let index = args
                    .iter()
                    .position(|argument| argument == "--append-system-prompt")
                    .expect("Claude orientation switch");
                assert_eq!(args.get(index + 1).map(String::as_str), Some(orientation));
            }
            Engine::Grok => {
                let index = args
                    .iter()
                    .position(|argument| argument == "--rules")
                    .expect("Grok orientation switch");
                assert_eq!(args.get(index + 1).map(String::as_str), Some(orientation));
            }
            Engine::Codex | Engine::Opencode => {
                assert!(
                    args.last()
                        .is_some_and(|prompt| prompt.contains(orientation))
                );
            }
            Engine::Kimi => unreachable!("Kimi keeps its agent-file contract"),
        }
    }
}

#[test]
fn explicit_tasks_keep_the_default_instruction_even_if_orientation_is_present() {
    let orientation = "orientation must not rewrite an explicit task";
    for engine in [
        Engine::Claude,
        Engine::Codex,
        Engine::Opencode,
        Engine::Grok,
    ] {
        let request = request(engine)
            .with_orientation(orientation)
            .with_initial_task("fixture task");
        let command = super::super::build(engine, OsStr::new(engine.as_str()), &request).unwrap();
        let args = command.args_lossy();
        assert!(!args.iter().any(|argument| argument == orientation));
        match engine {
            Engine::Claude => {
                let index = args
                    .iter()
                    .position(|argument| argument == "--append-system-prompt")
                    .expect("Claude instruction switch");
                assert_eq!(
                    args.get(index + 1).map(String::as_str),
                    Some(ORCHESTRATOR_DIRECTIVE)
                );
            }
            Engine::Grok => {
                let index = args
                    .iter()
                    .position(|argument| argument == "--rules")
                    .expect("Grok instruction switch");
                assert_eq!(
                    args.get(index + 1).map(String::as_str),
                    Some(ORCHESTRATOR_DIRECTIVE)
                );
            }
            Engine::Codex | Engine::Opencode => {
                assert!(
                    args.last()
                        .is_some_and(|prompt| prompt.contains(ORCHESTRATOR_DIRECTIVE))
                );
            }
            Engine::Kimi => unreachable!("Kimi keeps its agent-file contract"),
        }
    }
}
