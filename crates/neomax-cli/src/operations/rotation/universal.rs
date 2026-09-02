use anyhow::Result;
use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::continuation::RotationTrigger;
use serde_json::json;

use crate::context::RuntimeContext;
use crate::launch;
use crate::models;
use crate::operations::handoff;
use crate::parser;

use super::render;

pub(super) fn execute(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    if let Some(record) = super::live::current(launcher, args, context, false)? {
        let handoff_args = super::live::without_run_selector(args);
        return handoff::run_live(launcher, context, &handoff_args, &record, false);
    }
    if args.iter().any(|arg| arg == "--dry-run") {
        return render::no_op(
            "rotate",
            args,
            "dry-run requested; no credentials, workers, or provider processes were touched",
        );
    }
    if parser::value(args, "--run")?.is_none() {
        if let Some(engine) = current_engine(launcher, args)? {
            return handoff::run(launcher, context, &untracked_handoff_args(engine, args));
        }
    }
    let reports = launch::rotate(launcher, context, args, RotationTrigger::Manual)?;
    render::reports(
        launcher,
        "rotate",
        args,
        reports,
        json!({"selection": "active-runs", "trigger": "manual"}),
    )
}

fn current_engine(launcher: Launcher, args: &[String]) -> Result<Option<Engine>> {
    let explicit = parser::value(args, "--engine")?
        .map(|value| models::parse_engine(&value))
        .transpose()?;
    Ok(explicit
        .or(match launcher {
            Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => {
                Some(engine)
            }
            Launcher::Universal => None,
        })
        .or_else(|| {
            std::env::var("NEOMAX_ROLE")
                .ok()
                .and_then(|value| models::parse_engine(&value).ok())
        }))
}

fn untracked_handoff_args(engine: Engine, args: &[String]) -> Vec<String> {
    let mut handoff_args = vec![
        "--engine".into(),
        engine.to_string(),
        "--reason".into(),
        "manual /rotate".into(),
    ];
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--engine" || argument == "--reason" {
            index += 2;
            continue;
        }
        if argument.starts_with("--engine=") || argument.starts_with("--reason=") {
            index += 1;
            continue;
        }
        handoff_args.push(argument.clone());
        index += 1;
    }
    handoff_args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn dry_run_is_model_free_and_does_not_need_a_provider_profile() {
        let fixture = fixture();
        execute(
            Launcher::Universal,
            &["--dry-run".into(), "--json".into()],
            &fixture.context,
        )
        .unwrap();
    }
}
