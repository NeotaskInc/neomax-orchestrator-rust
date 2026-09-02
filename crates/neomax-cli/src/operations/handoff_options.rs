use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::handoff::{LaunchOptions, PreservedEnvironment, default_kickoff};
use neomax_core::orchestration::registry::OrchestratorRecord;

use crate::context::RuntimeContext;
use crate::models;
use crate::parser;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HandoffOptions {
    pub engine: Engine,
    pub source_account: Option<String>,
    pub target_selectors: Vec<String>,
    pub reason: String,
    pub reason_explicit: bool,
    pub cwd: PathBuf,
    pub kickoff: Option<String>,
    pub worker_scope: Option<String>,
    pub model_overrides: BTreeMap<Engine, String>,
    pub environment: PreservedEnvironment,
    pub headless: bool,
    pub check: bool,
    pub dry_run: bool,
    pub json: bool,
    pub run_id: Option<String>,
    pub session: Option<String>,
    pub interactive_only: bool,
}

impl HandoffOptions {
    pub(crate) fn for_live_orchestrator(
        &self,
        record: &OrchestratorRecord,
        worker_scope: Option<String>,
    ) -> Self {
        let mut options = self.clone();
        options.engine = record.engine;
        options.cwd = if record.cwd.as_os_str().is_empty() {
            self.cwd.clone()
        } else {
            record.cwd.clone()
        };
        options.source_account = record
            .account
            .map(|account| account.to_string())
            .or_else(|| options.source_account.clone());
        options.worker_scope = worker_scope
            .or_else(|| {
                record
                    .extra
                    .get("worker_scope")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .or_else(|| self.worker_scope.clone());
        options.session = Some(record.session.clone());
        options
            .environment
            .values
            .insert("NEOMAX_ORCH_SESSION".into(), record.session.clone());
        options.interactive_only = true;
        if !record.model.trim().is_empty() {
            options
                .model_overrides
                .entry(record.engine)
                .or_insert_with(|| record.model.clone());
        }
        let project = record
            .project
            .as_deref()
            .or_else(|| record_metadata_string(record, "project"));
        if let Some(project) = project {
            options
                .environment
                .values
                .insert("NEOMAX_PROJECT".into(), project.into());
        }
        let branch_prefix = record
            .branch_prefix
            .as_deref()
            .or_else(|| record_metadata_string(record, "branch_prefix"));
        if let Some(branch_prefix) = branch_prefix {
            options
                .environment
                .values
                .insert("NEOMAX_BRANCH_PREFIX".into(), branch_prefix.into());
        }
        options
    }

    pub(crate) fn launch_options(
        &self,
        source_account: &str,
        target_account: &str,
    ) -> LaunchOptions {
        LaunchOptions {
            engine: self.engine,
            source_account: source_account.into(),
            target_account: target_account.into(),
            reason: self.reason.clone(),
            cwd: self.cwd.clone(),
            kickoff: self
                .kickoff
                .clone()
                .unwrap_or_else(|| default_kickoff(self.engine, source_account)),
            worker_scope: self.worker_scope.clone(),
            model_overrides: self.model_overrides.clone(),
            environment: root_environment(&self.environment),
            headless: self.headless,
            // A same-provider handoff lands on a different isolated profile.
            // Its old provider session is not owned by the target account, so
            // the new root must use the installed Kimi agent file and adopt
            // the task from the durable run or handoff baton.
            session_id: None,
            resume: false,
        }
    }
}

fn root_environment(environment: &PreservedEnvironment) -> PreservedEnvironment {
    let mut environment = environment.clone();
    // The replacement launcher is a new human-facing Neomax root. Do not
    // carry an agent invocation marker into it; the root will build its own
    // canonical tool environment when it starts the provider.
    environment
        .values
        .remove(neomax_core::agent_tools::NEOMAX_TOOL_POLICY_ENV);
    environment
        .values
        .remove(neomax_core::agent_tools::NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV);
    environment
}

fn record_metadata_string<'a>(record: &'a OrchestratorRecord, key: &str) -> Option<&'a str> {
    record
        .extra
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
}

pub(crate) fn parse(
    launcher: Launcher,
    context: &RuntimeContext,
    args: &[String],
) -> Result<HandoffOptions> {
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let explicit_engine = parser::value(args, "--engine")?
        .map(|value| models::parse_engine(&value))
        .transpose()?;
    if let (Some(explicit), Some(pinned)) = (explicit_engine, launcher_engine(launcher)) {
        if explicit != pinned {
            bail!(
                "{} handoff is pinned to {pinned}; --engine {explicit} is not valid",
                match launcher {
                    Launcher::ProviderOrchestrator(engine) => engine,
                    Launcher::AccountHelper(engine) => engine,
                    Launcher::Universal => unreachable!(),
                }
            );
        }
    }
    let engine = explicit_engine
        .or_else(|| launcher_engine(launcher))
        .or_else(|| {
            environment
                .get("NEOMAX_ROLE")
                .and_then(|value| models::parse_engine(value).ok())
        })
        .or_else(|| {
            let identity = neomax_core::orchestration::handoff::identity(
                &environment,
                &context.paths.home,
                &context.cwd,
            );
            Some(identity.engine)
        })
        .ok_or_else(|| anyhow::anyhow!("handoff requires --engine in dynamic neomax mode"))?;

    let cwd = parser::value(args, "--base")?
        .map(|value| context.resolve_path(&value))
        .unwrap_or_else(|| context.cwd.clone());
    if !cwd.is_dir() {
        bail!("handoff base directory does not exist: {}", cwd.display());
    }

    let mut preserved = PreservedEnvironment::from_environment(&environment);
    if let Some(scope) = parser::value(args, "--workers")? {
        preserved.values.insert("NEOMAX_FLEET".into(), scope);
    }
    let mut model_overrides = preserved.model_overrides();
    let configured_models = context.model_overrides()?;
    for provider in Engine::ALL {
        if let Some(model) = configured_models.get(provider) {
            model_overrides.insert(provider, model.to_owned());
        }
    }
    for provider in Engine::ALL {
        let flag = format!("--{provider}-model");
        if let Some(model) = parser::value(args, &flag)? {
            model_overrides.insert(
                provider,
                models::validate_model_for_engine(provider, model)?,
            );
        }
    }
    if let Some(model) = parser::value(args, "--model")? {
        model_overrides.insert(engine, models::validate_model_for_engine(engine, model)?);
    }

    let reason_override = parser::value(args, "--reason")?;
    Ok(HandoffOptions {
        engine,
        source_account: parser::value(args, "--from")?
            .or(parser::value(args, "--source-account")?)
            .or_else(|| environment.get("NEOMAX_ACCOUNT").cloned()),
        target_selectors: target_selectors(args)?,
        reason: reason_override
            .clone()
            .unwrap_or_else(|| "manual handoff".into()),
        reason_explicit: reason_override.is_some(),
        cwd,
        kickoff: parser::value(args, "--kickoff")?.or(parser::value(args, "--prompt")?),
        worker_scope: preserved.worker_scope(),
        model_overrides,
        environment: preserved,
        headless: environment
            .get("SSH_CONNECTION")
            .is_some_and(|value| !value.is_empty())
            || environment
                .get("SSH_TTY")
                .is_some_and(|value| !value.is_empty()),
        check: parser::has(args, "--check"),
        dry_run: parser::has(args, "--dry-run"),
        json: parser::has(args, "--json"),
        run_id: parser::value(args, "--run")?,
        session: parser::value(args, "--session")?,
        interactive_only: false,
    })
}

fn target_selectors(args: &[String]) -> Result<Vec<String>> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let current = &args[index];
        let (flag, inline) = current
            .split_once('=')
            .map_or((current.as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        if matches!(flag, "--to" | "--target-account" | "--account") {
            if let Some(value) = inline {
                if value.is_empty() {
                    bail!("{flag} requires a value");
                }
                values.push(value.to_owned());
            } else {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))?;
                values.push(value.clone());
                index += 1;
            }
        }
        index += 1;
    }

    let positional_args = args
        .iter()
        .filter(|value| !matches!(value.as_str(), "--check" | "--dry-run" | "--json"))
        .cloned()
        .collect::<Vec<_>>();
    let positionals = parser::positional(
        &positional_args,
        &[
            "--engine",
            "--from",
            "--source-account",
            "--to",
            "--target-account",
            "--account",
            "--reason",
            "--kickoff",
            "--prompt",
            "--base",
            "--workers",
            "--model",
            "--run",
            "--session",
            "--claude-model",
            "--codex-model",
            "--opencode-model",
            "--kimi-model",
            "--grok-model",
        ],
    )?;
    values.extend(positionals);
    Ok(neomax_core::orchestration::handoff::parse_account_selectors(&values))
}

fn launcher_engine(launcher: Launcher) -> Option<Engine> {
    match launcher {
        Launcher::Universal => None,
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
    }
}

#[cfg(test)]
#[path = "handoff_options_tests.rs"]
mod tests;
