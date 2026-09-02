use anyhow::{Context, Result, bail};

use crate::context::RuntimeContext;

use super::super::commands::{models_command, provider_supports_models};
use super::super::process::ProcessPort;
use super::super::profiles::AuthPort;
use super::super::render::ActionOutput;
use super::super::request::AccountHelperRequest;
use super::shared::{ensure_success, report_output};

pub(super) fn execute(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
    auth: &dyn AuthPort,
    process: &dyn ProcessPort,
) -> Result<()> {
    if !provider_supports_models(request.engine) {
        bail!(
            "{} does not expose a local model-list command",
            request.engine
        );
    }
    let profile = auth.ensure_profile(
        request.engine,
        &request.account,
        &context.paths.home,
        &context.cwd,
    )?;
    let invocation = models_command(request, &profile, &context.paths.home, &context.cwd)
        .context("provider model discovery is unavailable")?;
    let outcome = process.invoke(&invocation)?;
    let report = ActionOutput {
        operation: "models",
        engine: request.engine.to_string(),
        account: profile.account().to_owned(),
        model: None,
        success: outcome.success,
        exit_code: outcome.status_code,
        stdout: String::from_utf8_lossy(&outcome.stdout).into_owned(),
        supported: true,
    };
    report_output(request, &report)?;
    ensure_success(&outcome, request.engine)
}
