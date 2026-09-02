use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

use crate::agent_tools::{LaunchRole, PreparedWorkerTools};
use crate::{Error, Result};

use super::{ProviderProcessSecret, ProviderProfile, catalog};

pub const DIRECTIVE: &str = "You are a headless delegated worker with full autonomy: never ask the user questions; decide and proceed. End with a concise factual report of what was done, what was verified, and any failures.";
pub const ORCHESTRATOR_DIRECTIVE: &str = "You are the Neomax orchestrator for this project. Preserve and follow the project's instructions. Use NEOMAX_BIN with the canonical commands in NEOMAX_TOOL_MANIFEST; inspect status, runs, usage, and account eligibility before routing work. Dispatch and coordinate workers across the configured scope, verify their results, and keep the task moving. Honor NEOMAX_TOOL_POLICY and NEOMAX_TOOL_DEPTH/NEOMAX_TOOL_MAX_DEPTH; never bypass the manifest or recursion policy. Do not behave as a delegated worker and do not ask the user to perform orchestration that the available tools can perform.";

#[derive(Clone, PartialEq, Eq)]
pub struct ProviderCommand {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: BTreeMap<OsString, OsString>,
    pub env_remove: Vec<OsString>,
    pub inherit_stdin: bool,
}

impl fmt::Debug for ProviderCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let environment = self
            .env
            .iter()
            .map(|(key, value)| {
                let key_text = key.to_string_lossy().into_owned();
                let value_text = if super::process_secret::is_secret_environment_key(&key_text) {
                    "<redacted>".into()
                } else {
                    value.to_string_lossy().into_owned()
                };
                (key_text, value_text)
            })
            .collect::<BTreeMap<_, _>>();
        formatter
            .debug_struct("ProviderCommand")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("cwd", &self.cwd)
            .field("env", &environment)
            .field("env_remove", &self.env_remove)
            .field("inherit_stdin", &self.inherit_stdin)
            .finish()
    }
}

impl ProviderCommand {
    pub fn new(program: impl Into<OsString>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: BTreeMap::new(),
            env_remove: Vec::new(),
            inherit_stdin: false,
        }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn remove_env(mut self, key: impl Into<OsString>) -> Self {
        self.env_remove.push(key.into());
        self
    }

    pub fn inherit_stdin(mut self) -> Self {
        self.inherit_stdin = true;
        self
    }

    pub(crate) fn scrub_ambient_secrets(mut self) -> Self {
        for (key, _) in std::env::vars_os() {
            if super::process_secret::is_secret_environment_key(&key.to_string_lossy()) {
                self = self.remove_env(key);
            }
        }
        self
    }

    pub(crate) fn inject_process_secret(
        mut self,
        engine: crate::Engine,
        secret: Option<&ProviderProcessSecret>,
    ) -> Self {
        if let Some(secret) = secret.filter(|secret| secret.engine() == engine) {
            self = self.env(secret.variable(), secret.value());
        }
        self
    }

    pub fn args_lossy(&self) -> Vec<String> {
        self.args
            .iter()
            .map(|item| item.to_string_lossy().into_owned())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct WorkerRequest {
    pub profile: ProviderProfile,
    pub model: Option<String>,
    pub prompt: String,
    pub goal: Option<String>,
    pub cwd: PathBuf,
    pub session_id: Option<String>,
    pub resume_session: Option<String>,
    pub effort: Option<String>,
    pub max_turns: Option<u32>,
    pub plan: bool,
    pub ultra: bool,
    pub config_home_override: Option<PathBuf>,
    pub agent_environment: BTreeMap<String, String>,
    pub(crate) process_secret: Option<ProviderProcessSecret>,
    launch_role: LaunchRole,
}

#[derive(Debug, Clone)]
pub struct WorkerLaunchContext {
    request: WorkerRequest,
    tools: PreparedWorkerTools,
}

impl WorkerLaunchContext {
    pub(crate) fn new(mut request: WorkerRequest, tools: PreparedWorkerTools) -> Self {
        request.launch_role = tools.role();
        for (key, value) in tools.variables() {
            request.agent_environment.insert(key.clone(), value.clone());
        }
        Self { request, tools }
    }

    pub fn request(&self) -> &WorkerRequest {
        &self.request
    }

    pub fn tools(&self) -> &PreparedWorkerTools {
        &self.tools
    }

    pub const fn role(&self) -> LaunchRole {
        self.tools.role()
    }

    #[cfg(test)]
    pub(crate) fn for_test(request: WorkerRequest) -> Self {
        Self::new(request, PreparedWorkerTools::test_fixture())
    }

    #[cfg(test)]
    pub(crate) fn for_test_role(request: WorkerRequest, role: LaunchRole) -> Self {
        Self::new(request, PreparedWorkerTools::test_fixture_for(role))
    }
}

impl WorkerRequest {
    pub fn new(
        profile: ProviderProfile,
        cwd: impl Into<PathBuf>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            profile,
            model: None,
            prompt: prompt.into(),
            goal: None,
            cwd: cwd.into(),
            session_id: None,
            resume_session: None,
            effort: None,
            max_turns: None,
            plan: false,
            ultra: false,
            config_home_override: None,
            agent_environment: BTreeMap::new(),
            process_secret: None,
            launch_role: LaunchRole::Worker,
        }
    }

    pub const fn launch_role(&self) -> LaunchRole {
        self.launch_role
    }

    pub(crate) fn set_launch_role(&mut self, role: LaunchRole) {
        self.launch_role = role;
    }

    pub fn selected_model(&self) -> &str {
        self.model
            .as_deref()
            .unwrap_or_else(|| catalog::default_model_id(self.profile.engine))
    }

    pub(crate) fn with_process_secret(mut self, secret: Option<ProviderProcessSecret>) -> Self {
        self.process_secret = secret;
        self
    }
}

pub(super) fn composed_prompt(context: &WorkerLaunchContext, plan_note: Option<&str>) -> String {
    let request = context.request();
    let directive = if context.role().is_orchestrator() {
        ORCHESTRATOR_DIRECTIVE
    } else {
        DIRECTIVE
    };
    let mut parts = vec![directive.to_string()];
    if let Some(goal) = request.goal.as_deref() {
        parts.push(goal_block(goal, request.max_turns));
    }
    if let Some(note) = plan_note {
        parts.push(note.into());
    }
    parts.push(request.prompt.clone());
    parts.join("\n\n")
}

pub(super) fn base_command(binary: &OsStr, context: &WorkerLaunchContext) -> ProviderCommand {
    let request = context.request();
    let provider = catalog::spec(request.profile.engine);
    let mut command = ProviderCommand::new(binary, &request.cwd).scrub_ambient_secrets();
    for key in provider.scrub.iter().map(String::as_str).chain([
        "NEOMAX_PROFILES",
        "NEOMAX_CODEX_PROFILES",
        "NEOMAX_OPENCODE_PROFILES",
        "NEOMAX_KIMI_PROFILES",
        "NEOMAX_GROK_PROFILES",
        "NEOMAX_CLAUDE_BIN",
        "NEOMAX_CODEX_BIN",
        "NEOMAX_OPENCODE_BIN",
        "NEOMAX_KIMI_BIN",
        "NEOMAX_GROK_BIN",
        "NEOMAX_NO_WORKTREE",
        "NEOMAX_WORKER",
        "NEOMAX_ORCHESTRATOR",
    ]) {
        command = command.remove_env(key);
    }
    for (key, value) in &request.agent_environment {
        if !super::process_secret::is_secret_environment_key(key) {
            command = command.env(key, value);
        }
    }
    command = command
        .inject_process_secret(request.profile.engine, request.process_secret.as_ref())
        .env(super::process_secret::PROCESS_SECRET_BOUNDARY_ENV, "1");
    if context.role().is_orchestrator() {
        command.env("NEOMAX_ORCHESTRATOR", "1")
    } else {
        command.env("NEOMAX_WORKER", "1")
    }
}

pub(super) fn apply_profile(
    mut command: ProviderCommand,
    request: &WorkerRequest,
    config_override: Option<&PathBuf>,
) -> Result<ProviderCommand> {
    let provider = catalog::spec(request.profile.engine);
    let home = current_home()?;
    let default = home.join(&provider.default_profile_dir);
    let selected = config_override.unwrap_or(&request.profile.path);
    if request.profile.path == default
        && provider.default_unsets_config_env
        && config_override.is_none()
    {
        command = command.remove_env(provider.config_env);
    } else {
        command = command.env(provider.config_env, selected.as_os_str());
    }
    Ok(command)
}

fn current_home() -> Result<PathBuf> {
    crate::runtime::RuntimeEnvironment::process()
        .home_dir()
        .ok_or_else(|| Error::InvalidArgument("HOME or USERPROFILE is not set".into()))
}

fn goal_block(goal: &str, max_turns: Option<u32>) -> String {
    let cap = max_turns.map_or_else(String::new, |turns| {
        format!(" Make at most {turns} rounds of self-correction; if the objective still is not met, stop and report exactly what remains.")
    });
    format!(
        "OBJECTIVE: do not finish until this condition holds:\n{goal}\nWork autonomously toward it, then VERIFY the condition is actually met by running the relevant checks, tests, or builds before you end. If verification fails, keep working until it passes.{cap}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;

    #[test]
    fn root_context_uses_orchestrator_policy_and_directive() {
        let profile = ProviderProfile {
            engine: Engine::Opencode,
            account: "fixture".into(),
            path: PathBuf::from("/profiles/opencode"),
            reserved: false,
        };
        let request = WorkerRequest::new(profile, "/tmp/work", "Inspect the project.");
        let context = WorkerLaunchContext::for_test_role(request, LaunchRole::Orchestrator);
        let command = base_command(OsStr::new("opencode"), &context);
        assert_eq!(
            command.env.get(OsStr::new("NEOMAX_TOOL_POLICY")),
            Some(&OsString::from("orchestrator"))
        );
        assert_eq!(
            command.env.get(OsStr::new("NEOMAX_ORCHESTRATOR")),
            Some(&OsString::from("1"))
        );
        assert!(!command.env.contains_key(OsStr::new("NEOMAX_WORKER")));
        assert!(composed_prompt(&context, None).contains(ORCHESTRATOR_DIRECTIVE));
    }

    #[test]
    fn provider_command_debug_redacts_secret_environment_values() {
        let command = ProviderCommand::new("fixture", "/tmp/work")
            .env("ANTHROPIC_API_KEY", "fixture-secret")
            .env("NEOMAX_ROLE", "worker");
        let debug = format!("{command:?}");
        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("worker"));
        assert!(!debug.contains("fixture-secret"));
    }
}
