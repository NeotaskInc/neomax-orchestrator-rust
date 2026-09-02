use anyhow::{Result, bail};
use neomax_core::Engine;

use crate::context::RuntimeContext;
use crate::output;

use super::super::process::ProcessOutcome;
use super::super::profiles::{AuthPort, ManagedProfile, profile_for};
use super::super::render::{ActionOutput, ActionReport};
use super::super::request::AccountHelperRequest;

pub(super) fn existing_profile(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
    auth: &dyn AuthPort,
) -> Result<ManagedProfile> {
    let profiles = auth.profiles(request.engine, &context.paths.home, &context.cwd)?;
    profile_for(&profiles, &request.account)
}

pub(super) fn effective_model(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
) -> Result<String> {
    let overrides = context.model_overrides()?;
    Ok(overrides
        .effective_model(request.engine, request.model.as_deref())?
        .model)
}

pub(super) fn report_output(request: &AccountHelperRequest, report: &ActionOutput) -> Result<()> {
    if request.json {
        output::json(report)?;
    } else {
        print!("{}", report.stdout);
        if !report.stdout.is_empty() && !report.stdout.ends_with('\n') {
            println!();
        }
    }
    Ok(())
}

pub(super) fn print_action(
    request: &AccountHelperRequest,
    report: &ActionReport,
    stdout: &[u8],
) -> Result<()> {
    super::super::render::print_action(request, report, stdout)
}

pub(super) fn ensure_success(outcome: &ProcessOutcome, engine: Engine) -> Result<()> {
    if outcome.success {
        return Ok(());
    }
    bail!(
        "{} exited unsuccessfully{}",
        engine,
        outcome
            .status_code
            .map_or_else(String::new, |code| format!(" with status {code}"))
    )
}
