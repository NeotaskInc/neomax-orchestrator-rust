use anyhow::{Context, Result, bail};
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::continuation::RotationTrigger;
use neomax_core::runs::RunStore;
use serde_json::json;

use super::render;
use crate::context::RuntimeContext;
use crate::error;
use crate::launch;
use crate::operations::handoff;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SessionOptions {
    runs: Vec<String>,
    sessions: Vec<String>,
    passthrough: Vec<String>,
}

pub(super) fn execute(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    let options = error::usage(SessionOptions::parse(args))?;
    let store = RunStore::new(&context.paths.runs);
    let all_runs = store.all()?;
    let mut selectors = options.runs.clone();
    selectors.extend(options.sessions.iter().cloned());
    if selectors.is_empty() {
        if let Some(session) = std::env::var("NEOMAX_ORCH_SESSION")
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            selectors.push(session);
        }
    }
    if selectors.is_empty() {
        if let Some(session) = std::env::var("NEOMAX_SESSION_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            selectors.push(session);
        }
    }
    if selectors.is_empty() {
        return render::no_op(
            "session-rotate",
            args,
            "no run or session selector was supplied and no active session identity is available",
        );
    }
    for selector in &selectors {
        if let Some(record) = super::live::session(launcher, args, context, selector)? {
            let mut handoff_args = Vec::new();
            if let Some(engine) = options
                .passthrough
                .iter()
                .position(|arg| arg == "--engine" || arg.starts_with("--engine="))
            {
                handoff_args.push(options.passthrough[engine].clone());
                if options.passthrough[engine] == "--engine"
                    && options.passthrough.get(engine + 1).is_some()
                {
                    handoff_args.push(options.passthrough[engine + 1].clone());
                }
            }
            handoff_args.extend(
                options
                    .passthrough
                    .iter()
                    .filter(|arg| matches!(arg.as_str(), "--json" | "--dry-run"))
                    .cloned(),
            );
            handoff_args.extend(["--session".into(), selector.clone()]);
            return handoff::run_live(launcher, context, &handoff_args, &record, false);
        }
    }
    let ids = select_run_ids(&all_runs, &selectors)?;
    if ids.is_empty() {
        return render::no_op(
            "session-rotate",
            args,
            "the requested run or session is not present in the local run ledger",
        );
    }
    let mut launch_args = options.passthrough;
    launch_args.extend(ids);
    let reports = launch::rotate(launcher, context, &launch_args, RotationTrigger::Manual)?;
    render::reports(
        launcher,
        "session-rotate",
        args,
        reports,
        json!({"selection": "session-or-run", "trigger": "manual"}),
    )
}

impl SessionOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self::default();
        let mut index = 0;
        while index < args.len() {
            let current = &args[index];
            let (flag, inline) = current
                .split_once('=')
                .map_or((current.as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            match flag {
                "--json" | "--all" | "--active" => options.passthrough.push(current.clone()),
                "--engine" | "--workers" => {
                    options.passthrough.push(current.clone());
                    if inline.is_none() {
                        let value = args
                            .get(index + 1)
                            .with_context(|| format!("{flag} requires a value"))?;
                        options.passthrough.push(value.clone());
                        index += 1;
                    }
                }
                "--run" | "--session" | "--session-id" => {
                    let value = option_value(args, &mut index, flag, inline)?;
                    if flag == "--run" {
                        options.runs.push(value);
                    } else {
                        options.sessions.push(value);
                    }
                }
                value if value.starts_with('-') => {
                    bail!("session-rotate: unknown option {current}")
                }
                value => options.runs.push(value.to_owned()),
            }
            index += 1;
        }
        Ok(options)
    }
}

fn option_value(
    args: &[String],
    index: &mut usize,
    flag: &str,
    inline: Option<&str>,
) -> Result<String> {
    if let Some(value) = inline {
        if value.is_empty() {
            bail!("{flag} requires a value");
        }
        return Ok(value.to_owned());
    }
    let value = args
        .get(*index + 1)
        .with_context(|| format!("{flag} requires a value"))?;
    *index += 1;
    Ok(value.clone())
}

fn select_run_ids(
    runs: &[neomax_core::runs::RunRecord],
    selectors: &[String],
) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    for selector in selectors {
        let matches = runs.iter().filter(|run| {
            run.id == *selector
                || run.session.as_deref() == Some(selector.as_str())
                || run.orch_session.as_deref() == Some(selector.as_str())
                || run
                    .session_history
                    .iter()
                    .any(|entry| entry.session == *selector)
        });
        for run in matches {
            if !ids.contains(&run.id) {
                ids.push(run.id.clone());
            }
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::Engine;
    use neomax_core::runs::RunStatus;
    use std::path::PathBuf;

    fn run(id: &str, session: Option<&str>) -> neomax_core::runs::RunRecord {
        let mut run = neomax_core::runs::RunRecord::new(
            id,
            Engine::Claude,
            "fixture-model",
            "fixture",
            PathBuf::from("/profiles/one"),
            PathBuf::from("/workspace"),
            1,
        );
        run.status = RunStatus::Limit;
        run.session = session.map(str::to_owned);
        run
    }

    #[test]
    fn session_selector_matches_run_session_and_deduplicates() {
        let runs = vec![run("run-1", Some("session-a")), run("run-2", None)];
        assert_eq!(
            select_run_ids(&runs, &["session-a".into(), "run-1".into()]).unwrap(),
            vec!["run-1"]
        );
    }

    #[test]
    fn missing_session_is_a_noop_without_provider_execution() {
        let fixture = crate::tests::fixture();
        execute(
            Launcher::Universal,
            &["--session".into(), "missing".into(), "--json".into()],
            &fixture.context,
        )
        .unwrap();
    }
}
