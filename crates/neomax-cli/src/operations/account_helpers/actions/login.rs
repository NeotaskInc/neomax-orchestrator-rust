use anyhow::{Result, bail};
use neomax_core::Engine;

use crate::context::RuntimeContext;

use super::super::commands::login_command;
use super::super::process::ProcessPort;
use super::super::profiles::AuthPort;
use super::super::render::ActionReport;
use super::super::request::{AccountHelperRequest, AccountOperation, AuthMode};
use super::shared::{ensure_success, print_action};

pub(super) fn execute(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
    auth: &dyn AuthPort,
    process: &dyn ProcessPort,
) -> Result<()> {
    let auth_mode = resolved_mode(request, auth)?;
    if matches!(request.engine, Engine::Grok | Engine::Kimi) && auth_mode == AuthMode::ApiKey {
        return configure_api_key(request, context, auth);
    }

    let profile = auth.ensure_profile(
        request.engine,
        &request.account,
        &context.paths.home,
        &context.cwd,
    )?;
    if request.engine == Engine::Grok {
        auth.set_preferred_auth(request.engine, &profile, auth_mode)?;
    }
    let request = request.with_auth_mode(auth_mode);
    let invocation = login_command(&request, &profile, &context.paths.home, &context.cwd)?;
    let outcome = process.invoke(&invocation)?;
    let report =
        ActionReport::from_outcome(&request, &profile, "login", None, &invocation, &outcome);
    print_action(&request, &report, &outcome.stdout)?;
    ensure_success(&outcome, request.engine)
}

fn configure_api_key(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
    auth: &dyn AuthPort,
) -> Result<()> {
    let secret = auth.api_key(request.engine)?;
    let profile = auth.configure_api_key(
        request.engine,
        &request.account,
        &context.paths.home,
        &context.cwd,
        &secret,
    )?;
    drop(secret);
    let report = ActionReport {
        operation: "login",
        engine: request.engine.to_string(),
        account: profile.account().to_owned(),
        model: None,
        success: true,
        exit_code: Some(0),
        command: format!("stored {} API-key profile", request.engine),
    };
    print_action(request, &report, &[])
}

fn resolved_mode(request: &AccountHelperRequest, auth: &dyn AuthPort) -> Result<AuthMode> {
    let AccountOperation::Login { auth_mode, .. } = &request.operation else {
        bail!("login request requires a login operation")
    };
    if request.engine == Engine::Grok && *auth_mode == AuthMode::Choose {
        if request.json {
            return Ok(AuthMode::OAuth);
        }
        return auth.choose_auth_mode(request.engine);
    }
    if request.engine == Engine::Kimi && *auth_mode == AuthMode::Choose {
        return Ok(AuthMode::OAuth);
    }
    Ok(*auth_mode)
}
