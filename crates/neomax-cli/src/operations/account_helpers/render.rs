use std::path::Path;

use anyhow::Result;
use neomax_core::Engine;
use serde::Serialize;

use crate::output;

use super::process::{ProcessInvocation, ProcessOutcome};
use super::profiles::{DetectedAuth, ManagedProfile};
use super::request::AccountHelperRequest;

#[derive(Debug, Serialize)]
pub(super) struct ActionReport {
    pub(super) operation: &'static str,
    pub(super) engine: String,
    pub(super) account: String,
    pub(super) model: Option<String>,
    pub(super) success: bool,
    pub(super) exit_code: Option<i32>,
    pub(super) command: String,
}

impl ActionReport {
    pub(super) fn from_outcome(
        request: &AccountHelperRequest,
        profile: &ManagedProfile,
        operation: &'static str,
        model: Option<String>,
        invocation: &ProcessInvocation,
        outcome: &ProcessOutcome,
    ) -> Self {
        Self {
            operation,
            engine: request.engine.to_string(),
            account: profile.account().to_owned(),
            model,
            success: outcome.success,
            exit_code: outcome.status_code,
            command: command_display(invocation),
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct ActionOutput {
    pub(super) operation: &'static str,
    pub(super) engine: String,
    pub(super) account: String,
    pub(super) model: Option<String>,
    pub(super) success: bool,
    pub(super) exit_code: Option<i32>,
    pub(super) stdout: String,
    pub(super) supported: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct StatusReport {
    pub(super) engine: String,
    pub(super) default_model: String,
    pub(super) auth_methods: Vec<&'static str>,
    pub(super) profiles: Vec<ProfileView>,
    pub(super) warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ProfileView {
    pub(super) account: String,
    pub(super) path: String,
    pub(super) authenticated: bool,
    pub(super) auth_method: Option<DetectedAuth>,
    pub(super) identity: Option<String>,
    pub(super) live_workers: u32,
    pub(super) cooldown_until: Option<i64>,
}

pub(super) fn print_action(
    request: &AccountHelperRequest,
    report: &ActionReport,
    stdout: &[u8],
) -> Result<()> {
    if request.json {
        return output::json(report);
    }
    println!(
        "{} account {} {}",
        report.engine, report.account, report.operation
    );
    if !stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(stdout));
    }
    Ok(())
}

pub(super) fn display_path(path: &Path, home: &Path) -> String {
    path.strip_prefix(home).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |relative| {
            if relative.as_os_str().is_empty() {
                "$HOME".into()
            } else {
                format!("$HOME/{}", relative.display())
            }
        },
    )
}

pub(super) fn helper_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Codex => "cdx",
        Engine::Opencode => "ocx",
        Engine::Kimi => "kmx",
        Engine::Grok => "gmx",
        Engine::Claude => "claude-helper",
    }
}

fn command_display(invocation: &ProcessInvocation) -> String {
    let mut values = vec![invocation.program.to_string_lossy().into_owned()];
    values.extend(
        invocation
            .args
            .iter()
            .map(|value| value.to_string_lossy().into_owned()),
    );
    values.join(" ")
}
