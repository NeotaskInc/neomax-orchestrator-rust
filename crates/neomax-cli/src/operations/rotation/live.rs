use std::path::{Path, PathBuf};

use anyhow::Result;
use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::registry::{OrchestratorRecord, OrchestratorStore};
use neomax_core::runs::{ProcessProbe, SystemProcessProbe};

use crate::context::RuntimeContext;
use crate::models;
use crate::parser;

pub(super) fn current(
    launcher: Launcher,
    args: &[String],
    context: &RuntimeContext,
    allow_positionals: bool,
) -> Result<Option<OrchestratorRecord>> {
    current_with_probe(
        launcher,
        args,
        context,
        allow_positionals,
        &SystemProcessProbe,
    )
}

fn current_with_probe(
    launcher: Launcher,
    args: &[String],
    context: &RuntimeContext,
    allow_positionals: bool,
    probe: &impl ProcessProbe,
) -> Result<Option<OrchestratorRecord>> {
    let requested_run = run_selector(args)?;
    let requested_engine = parser::value(args, "--engine")?
        .map(|value| models::parse_engine(&value))
        .transpose()?
        .or_else(|| launcher_engine(launcher))
        .or_else(|| {
            std::env::var("NEOMAX_ROLE")
                .ok()
                .and_then(|value| models::parse_engine(&value).ok())
        });
    let requested_session = parser::value(args, "--session")?
        .or(parser::value(args, "--session-id")?)
        .or_else(|| std::env::var("NEOMAX_ORCH_SESSION").ok())
        .filter(|value| !value.trim().is_empty());
    let records = OrchestratorStore::new(&context.paths.orchestrators).live(probe, context.now)?;
    let mut matches = records
        .into_iter()
        .filter(|record| requested_engine.is_none_or(|engine| record.engine == engine))
        .filter(|record| record.cwd.as_os_str().is_empty() || same_path(&record.cwd, &context.cwd))
        .filter(|record| {
            requested_session
                .as_deref()
                .is_none_or(|session| record.session == session)
        })
        .filter(|record| {
            requested_run.as_deref().is_none_or(|run_id| {
                record
                    .extra
                    .get("run_id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|record_run| record_run == run_id)
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .last_seen
            .cmp(&left.last_seen)
            .then_with(|| left.session.cmp(&right.session))
    });
    if !allow_positionals && has_run_selector(args) && requested_run.is_some() {
        return Ok((matches.len() == 1).then(|| matches.remove(0)));
    }
    Ok((matches.len() == 1).then(|| matches.remove(0)))
}

pub(super) fn session(
    launcher: Launcher,
    args: &[String],
    context: &RuntimeContext,
    selector: &str,
) -> Result<Option<OrchestratorRecord>> {
    let mut scoped = args.to_vec();
    if !scoped
        .iter()
        .any(|arg| arg == "--session" || arg.starts_with("--session="))
    {
        scoped.extend(["--session".into(), selector.into()]);
    }
    current(launcher, &scoped, context, true)
}

pub(super) fn without_run_selector(args: &[String]) -> Vec<String> {
    let value_flags = ["--engine", "--workers", "--session", "--session-id"];
    let mut output = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let flag = arg.split('=').next().unwrap_or(arg);
        if flag == "--run" {
            if !arg.contains('=') {
                index += 1;
            }
        } else if value_flags.contains(&flag) {
            output.push(args[index].clone());
            if !arg.contains('=') {
                if let Some(value) = args.get(index + 1) {
                    output.push(value.clone());
                    index += 1;
                }
            }
        } else if arg.starts_with('-') {
            output.push(args[index].clone());
        }
        index += 1;
    }
    output
}

fn has_run_selector(args: &[String]) -> bool {
    let value_flags = [
        "--engine",
        "--workers",
        "--run",
        "--session",
        "--session-id",
    ];
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let flag = arg.split('=').next().unwrap_or(arg);
        if value_flags.contains(&flag) {
            if !arg.contains('=') {
                index += 1;
            }
        } else if !arg.starts_with('-') {
            return true;
        }
        index += 1;
    }
    parser::has(args, "--run")
}

fn run_selector(args: &[String]) -> Result<Option<String>> {
    if let Some(value) = parser::value(args, "--run")? {
        return Ok(Some(value));
    }
    let value_flags = [
        "--engine",
        "--workers",
        "--run",
        "--session",
        "--session-id",
    ];
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let flag = arg.split('=').next().unwrap_or(arg);
        if value_flags.contains(&flag) {
            if !arg.contains('=') {
                index += 1;
            }
        } else if !arg.starts_with('-') {
            return Ok(Some(arg.to_owned()));
        }
        index += 1;
    }
    Ok(None)
}

fn launcher_engine(launcher: Launcher) -> Option<Engine> {
    match launcher {
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
        Launcher::Universal => None,
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize(left) == normalize(right)
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::RootDir
            | std::path::Component::Prefix(_)
            | std::path::Component::Normal(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;
    use neomax_core::Engine;
    use neomax_core::orchestration::registry::OrchestratorRegistration;

    struct LiveProbe;

    impl ProcessProbe for LiveProbe {
        fn pid_alive(&self, _pid: u32) -> bool {
            true
        }

        fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
            true
        }
    }

    fn register(fixture: &crate::tests::Fixture, session: &str) {
        OrchestratorStore::new(&fixture.context.paths.orchestrators)
            .register(OrchestratorRegistration {
                session: session.into(),
                pid: Some(std::process::id()),
                engine: Engine::Opencode,
                account: Some(1),
                account_dir: ".opencode".into(),
                project: Some("fixture-project".into()),
                branch_prefix: Some("fixture".into()),
                cwd: fixture.context.cwd.clone(),
                model: "opencode/big-pickle".into(),
                reserved: false,
                now: fixture.context.now,
            })
            .unwrap();
    }

    #[test]
    fn selects_the_live_registry_session_without_consulting_run_records() {
        let fixture = fixture();
        register(&fixture, "interactive");
        let selected = current_with_probe(
            Launcher::ProviderOrchestrator(Engine::Opencode),
            &[],
            &fixture.context,
            false,
            &LiveProbe,
        )
        .unwrap()
        .unwrap();
        assert_eq!(selected.session, "interactive");
        assert_eq!(selected.project.as_deref(), Some("fixture-project"));
        assert_eq!(selected.branch_prefix.as_deref(), Some("fixture"));
    }

    #[test]
    fn explicit_run_ids_keep_the_headless_run_path() {
        let fixture = fixture();
        register(&fixture, "interactive");
        assert!(
            current(
                Launcher::ProviderOrchestrator(Engine::Opencode),
                &["run-1".into()],
                &fixture.context,
                false,
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn session_selector_is_exact_and_does_not_guess_between_live_tuis() {
        let fixture = fixture();
        register(&fixture, "interactive");
        let selected = session(
            Launcher::ProviderOrchestrator(Engine::Opencode),
            &[],
            &fixture.context,
            "missing",
        )
        .unwrap();
        assert!(selected.is_none());
    }

    #[test]
    fn run_selector_is_removed_before_building_an_interactive_handoff() {
        assert_eq!(
            without_run_selector(&[
                "run-1".into(),
                "--workers".into(),
                "claude,opencode".into(),
                "--json".into(),
            ]),
            vec![
                "--workers".to_owned(),
                "claude,opencode".to_owned(),
                "--json".to_owned()
            ]
        );
        assert_eq!(
            without_run_selector(&["--run=run-1".into(), "--json".into()]),
            vec!["--json"]
        );
    }
}
