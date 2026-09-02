use anyhow::Result;
use neomax_core::Engine;
use neomax_core::providers::catalog::{self, CodexAuthIdentity, GrokAuthIdentity, RealFileSystem};

use crate::context::RuntimeContext;

use super::super::commands::whoami_command;
use super::super::process::{ProcessOutcome, ProcessPort};
use super::super::profiles::{AuthPort, ManagedProfile};
use super::super::render::ActionOutput;
use super::super::request::AccountHelperRequest;
use super::shared::{ensure_success, existing_profile, report_output};

pub(super) fn execute(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
    auth: &dyn AuthPort,
    process: &dyn ProcessPort,
) -> Result<()> {
    let profile = existing_profile(request, context, auth)?;
    if request.engine == Engine::Grok {
        let identity = grok_identity(&profile);
        let stdout = grok_whoami_output(&profile, identity.as_ref());
        let success = profile.authenticated() || identity.is_some();
        let report = ActionOutput {
            operation: "whoami",
            engine: request.engine.to_string(),
            account: profile.account().to_owned(),
            model: None,
            success,
            exit_code: Some(if success { 0 } else { 1 }),
            stdout,
            supported: true,
        };
        report_output(request, &report)?;
        if success {
            return Ok(());
        }
        return ensure_success(
            &ProcessOutcome {
                status_code: Some(1),
                success: false,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
            request.engine,
        );
    }
    let Some(invocation) = whoami_command(request, &profile, &context.paths.home, &context.cwd)
    else {
        let report = ActionOutput {
            operation: "whoami",
            engine: request.engine.to_string(),
            account: profile.account().to_owned(),
            model: None,
            success: profile.authenticated(),
            exit_code: None,
            stdout: profile.auth.map_or_else(
                || "not authenticated".into(),
                |method| method.label().into(),
            ),
            supported: true,
        };
        report_output(request, &report)?;
        return Ok(());
    };
    let outcome = process.invoke(&invocation)?;
    let stdout = if request.engine == Engine::Codex {
        codex_whoami_output(&profile, &outcome)
    } else {
        String::from_utf8_lossy(&outcome.stdout).into_owned()
    };
    let report = ActionOutput {
        operation: "whoami",
        engine: request.engine.to_string(),
        account: profile.account().to_owned(),
        model: None,
        success: outcome.success,
        exit_code: outcome.status_code,
        stdout,
        supported: true,
    };
    report_output(request, &report)?;
    ensure_success(&outcome, request.engine)
}

pub(super) fn codex_identity(profile: &ManagedProfile) -> Option<CodexAuthIdentity> {
    (profile.profile.engine == Engine::Codex)
        .then(|| catalog::codex_auth_identity(&profile.profile.path, &RealFileSystem))?
}

pub(super) fn grok_identity(profile: &ManagedProfile) -> Option<GrokAuthIdentity> {
    (profile.profile.engine == Engine::Grok)
        .then(|| catalog::grok_auth_identity(&profile.profile.path, &RealFileSystem))?
}

pub(super) fn grok_whoami_output(
    profile: &ManagedProfile,
    identity: Option<&GrokAuthIdentity>,
) -> String {
    let mut lines = Vec::new();
    if let Some(identity) = identity {
        lines.push(format!("method: {}", identity.method()));
        if let Some(email) = identity.email() {
            lines.push(format!("email: {email}"));
        }
        if let Some(name) = identity.name() {
            lines.push(format!("name: {name}"));
        }
        if let Some(team) = identity.team() {
            lines.push(format!("team: {team}"));
        }
    } else if let Some(method) = profile.auth {
        lines.push(format!("authenticated via {}", method.label()));
    } else {
        lines.push("not authenticated".into());
    }
    let mut output = lines.join("\n");
    output.push('\n');
    output
}

pub(super) fn codex_whoami_output(profile: &ManagedProfile, outcome: &ProcessOutcome) -> String {
    let mut lines = vec![match profile.auth {
        Some(method) => format!("authenticated via {}", method.label()),
        None => "not authenticated".into(),
    }];
    if let Some(identity) = codex_identity(profile) {
        lines.push(format!("account identity {}", identity.label()));
        if let Some(plan) = identity.plan() {
            lines.push(format!("plan {plan}"));
        }
    }
    if !outcome.success {
        lines.push("provider status command failed; local credential state shown".into());
    }
    let mut output = lines.join("\n");
    output.push('\n');
    output
}
