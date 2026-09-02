use anyhow::{Result, bail};
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::continuation::RotationTrigger;
use serde_json::json;

use crate::context::RuntimeContext;
use crate::error;
use crate::launch;
use crate::operations::handoff;
use crate::parser;

#[path = "tick/armed.rs"]
mod armed;

use super::render;

pub(super) fn execute(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    error::usage(validate_args(args))?;
    if parser::has(args, "--dry-run") {
        return render::no_op(
            "rotate-tick",
            args,
            "dry-run requested; no credentials, workers, or provider processes were touched",
        );
    }
    let active = parser::has(args, "--active");
    let armed = armed::sweep(launcher, args, context, active)?;
    if let Some(record) = super::live::current(launcher, args, context, false)? {
        if !armed.handled_sessions.contains(&record.session) {
            let handoff_args = args
                .iter()
                .filter(|arg| !matches!(arg.as_str(), "--active" | "--all"))
                .cloned()
                .collect::<Vec<_>>();
            handoff::run_live(launcher, context, &handoff_args, &record, true)?;
        }
    }
    let reports = launch::rotate_model_free(launcher, context, args, RotationTrigger::Tick)?;
    render::reports(
        launcher,
        "rotate-tick",
        args,
        reports,
        json!({
            "selection": if parser::has(args, "--active") { "active-runs" } else { "quota-limited-runs" },
            "trigger": "tick",
            "model_free": true,
            "active": active,
            "armed": armed.reports,
        }),
    )
}

fn validate_args(args: &[String]) -> Result<()> {
    let mut index = 0;
    while index < args.len() {
        let current = &args[index];
        let (flag, inline) = current
            .split_once('=')
            .map_or((current.as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        match flag {
            "--active" | "--json" | "--all" | "--dry-run" => {}
            "--workers" | "--engine" | "--run" => {
                if inline.is_none() {
                    if args.get(index + 1).is_none() {
                        bail!("{flag} requires a value");
                    }
                    index += 1;
                } else if inline.is_some_and(str::is_empty) {
                    bail!("{flag} requires a value");
                }
            }
            value if value.starts_with('-') => bail!("rotate-tick: unknown option {current}"),
            _ => {}
        }
        index += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn tick_defaults_to_quota_limited_runs_without_provider_execution() {
        let fixture = fixture();
        execute(Launcher::Universal, &["--json".into()], &fixture.context).unwrap();
    }

    #[test]
    fn active_tick_is_explicitly_model_free() {
        let fixture = fixture();
        execute(
            Launcher::Universal,
            &["--active".into(), "--json".into()],
            &fixture.context,
        )
        .unwrap();
    }

    #[test]
    fn tick_rejects_unknown_options_before_runtime_access() {
        assert!(validate_args(&["--provider-call".into()]).is_err());
    }
}
