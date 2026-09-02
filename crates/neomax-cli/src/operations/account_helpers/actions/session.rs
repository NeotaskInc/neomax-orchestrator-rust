use anyhow::{Result, bail};

use crate::context::RuntimeContext;

use super::super::commands::{logout_command, run_command};
use super::super::process::ProcessPort;
use super::super::profiles::AuthPort;
use super::super::render::ActionReport;
use super::super::request::AccountHelperRequest;
use super::shared::{effective_model, ensure_success, existing_profile, print_action};

pub(super) fn logout(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
    auth: &dyn AuthPort,
    process: &dyn ProcessPort,
) -> Result<()> {
    let profile = existing_profile(request, context, auth)?;
    let invocation = logout_command(request, &profile, &context.paths.home, &context.cwd)?;
    let outcome = process.invoke(&invocation)?;
    let report =
        ActionReport::from_outcome(request, &profile, "logout", None, &invocation, &outcome);
    print_action(request, &report, &outcome.stdout)?;
    ensure_success(&outcome, request.engine)
}

pub(super) fn run(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
    auth: &dyn AuthPort,
    process: &dyn ProcessPort,
) -> Result<()> {
    let profile = existing_profile(request, context, auth)?;
    if !profile.authenticated() {
        bail!(
            "{} account {} is not authenticated; run {} login {} first",
            request.engine,
            profile.account(),
            super::super::render::helper_name(request.engine),
            profile.account()
        );
    }
    let model = effective_model(request, context)?;
    let invocation = run_command(request, &profile, &model, &context.paths.home, &context.cwd)?;
    let outcome = process.invoke(&invocation)?;
    let report =
        ActionReport::from_outcome(request, &profile, "run", Some(model), &invocation, &outcome);
    print_action(request, &report, &outcome.stdout)?;
    ensure_success(&outcome, request.engine)
}
