use anyhow::Result;
use neomax_core::orchestration::commands::Launcher;

use crate::context::RuntimeContext;
use crate::error;
use crate::output;

use super::{LaunchOptions, build_plan, detached, execute, options, render, resume};

pub(crate) fn run(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    let options = error::usage(LaunchOptions::parse(launcher, args))?;
    let options = resume::resolve(launcher, options, context)?;
    error::usage(options::validate(launcher, &options))?;
    if !options.dry_run
        && options.detach
        && !matches!(launcher, Launcher::AccountHelper(_))
        && std::env::var_os("NEOMAX_ATTACHED_CHILD").is_none()
    {
        emit_thin_brief_warning(&options);
        return detached::spawn(launcher, args, context);
    }
    emit_thin_brief_warning(&options);
    if render::plan_is_json(args) {
        if options.dry_run {
            let plan = build_plan(launcher, options, context)?;
            output::json(&plan)
        } else {
            execute::run(launcher, options, context, true)
        }
    } else if options.dry_run {
        let plan = build_plan(launcher, options, context)?;
        render::print_text(&plan);
        Ok(())
    } else {
        execute::run(launcher, options, context, false)
    }
}

fn emit_thin_brief_warning(options: &LaunchOptions) {
    // The parent owns the advisory because a detached child has no user-facing stderr.
    if std::env::var_os("NEOMAX_ATTACHED_CHILD").is_none() {
        if let Some(warning) = super::validation::thin_brief_warning(options) {
            eprintln!("{warning}");
        }
    }
}
