use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{Engine, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreservedEnvironment {
    pub values: BTreeMap<String, String>,
}

impl PreservedEnvironment {
    pub fn from_environment(environment: &BTreeMap<String, String>) -> Self {
        let keys = [
            "NEOMAX_FLEET",
            "NEOMAX_DEFAULT_MODEL",
            "NEOMAX_CLAUDE_MODEL",
            "NEOMAX_CODEX_MODEL",
            "NEOMAX_OPENCODE_MODEL",
            "NEOMAX_KIMI_MODEL",
            "NEOMAX_GROK_MODEL",
            crate::agent_tools::NEOMAX_TOOL_POLICY_ENV,
            crate::agent_tools::NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV,
        ];
        Self {
            values: keys
                .into_iter()
                .filter_map(|key| {
                    environment
                        .get(key)
                        .map(|value| (key.into(), value.clone()))
                })
                .collect(),
        }
    }

    pub fn model_overrides(&self) -> BTreeMap<Engine, String> {
        [
            (Engine::Claude, "NEOMAX_CLAUDE_MODEL"),
            (Engine::Codex, "NEOMAX_CODEX_MODEL"),
            (Engine::Opencode, "NEOMAX_OPENCODE_MODEL"),
            (Engine::Kimi, "NEOMAX_KIMI_MODEL"),
            (Engine::Grok, "NEOMAX_GROK_MODEL"),
        ]
        .into_iter()
        .filter_map(|(engine, key)| {
            self.values
                .get(key)
                .filter(|value| !value.trim().is_empty())
                .map(|value| (engine, value.clone()))
        })
        .collect()
    }

    pub fn worker_scope(&self) -> Option<String> {
        self.values
            .get("NEOMAX_FLEET")
            .filter(|value| !value.trim().is_empty())
            .cloned()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaunchOptions {
    pub engine: Engine,
    pub source_account: String,
    pub target_account: String,
    pub reason: String,
    pub cwd: PathBuf,
    pub kickoff: String,
    pub worker_scope: Option<String>,
    pub model_overrides: BTreeMap<Engine, String>,
    pub environment: PreservedEnvironment,
    pub headless: bool,
    /// An existing provider session to resume when the provider supports a
    /// safe session-level handoff. Kimi is the interactive provider that
    /// requires this field instead of a positional startup task.
    pub session_id: Option<String>,
    pub resume: bool,
}

impl LaunchOptions {
    pub fn from_environment(
        engine: Engine,
        source_account: impl Into<String>,
        target_account: impl Into<String>,
        reason: impl Into<String>,
        cwd: impl Into<PathBuf>,
        kickoff: impl Into<String>,
        environment: &BTreeMap<String, String>,
    ) -> Self {
        let preserved = PreservedEnvironment::from_environment(environment);
        Self {
            engine,
            source_account: source_account.into(),
            target_account: target_account.into(),
            reason: reason.into(),
            cwd: cwd.into(),
            kickoff: kickoff.into(),
            worker_scope: preserved.worker_scope(),
            model_overrides: preserved.model_overrides(),
            environment: preserved,
            headless: environment
                .get("SSH_CONNECTION")
                .is_some_and(|value| !value.is_empty())
                || environment
                    .get("SSH_TTY")
                    .is_some_and(|value| !value.is_empty()),
            session_id: environment
                .get("NEOMAX_ORCH_SESSION")
                .filter(|value| !value.trim().is_empty())
                .cloned(),
            resume: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaunchPlan {
    pub engine: Engine,
    pub launcher: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub environment: BTreeMap<String, String>,
    /// A copyable command preview for humans and JSON output only. Process
    /// launches must use `launcher`, `args`, and `cwd` as typed fields.
    pub shell_command: String,
    pub headless: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Posix,
    PowerShell,
    /// `cmd.exe` batch-file syntax. This remains a display preview; process
    /// execution never passes through a shell.
    Cmd,
}

impl ShellKind {
    pub const fn host() -> Self {
        if cfg!(windows) {
            Self::PowerShell
        } else {
            Self::Posix
        }
    }
}

pub fn build_launch_plan(options: &LaunchOptions) -> Result<LaunchPlan> {
    if options.target_account.trim().is_empty() {
        return Err(Error::InvalidArgument(
            "handoff target account is empty".into(),
        ));
    }
    if options.cwd.as_os_str().is_empty() {
        return Err(Error::InvalidArgument(
            "handoff working directory is empty".into(),
        ));
    }
    let mut args = Vec::new();
    if options.target_account.eq_ignore_ascii_case("orch") {
        args.push("--orchestrator".into());
    } else {
        args.push(options.target_account.clone());
    }
    if let Some(scope) = &options.worker_scope {
        args.extend(["--workers".into(), scope.clone()]);
    }
    for engine in Engine::ALL {
        if let Some(model) = options.model_overrides.get(&engine) {
            let model = crate::settings::resolve_explicit_model(engine, model)?;
            args.extend([format!("--{engine}-model"), model]);
        }
    }
    if options.engine == Engine::Kimi {
        if options.resume {
            let session = options
                .session_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    Error::InvalidArgument(
                        "Kimi handoff resume requires a durable session ID".into(),
                    )
                })?;
            args.extend(["--session-id".into(), session.into(), "--resume".into()]);
        }
        // Kimi's interactive root rejects positional startup tasks. Its
        // installed agent file and durable Neomax state provide the handoff
        // context; the task remains in the run/baton before this process is
        // started.
    } else {
        args.push(normalize_kickoff(&options.kickoff));
    }
    let launcher = launcher_for(options.engine).to_string();
    let shell_command = render_shell_command(&options.cwd, &launcher, &args);
    Ok(LaunchPlan {
        engine: options.engine,
        launcher,
        args,
        cwd: options.cwd.clone(),
        environment: options.environment.values.clone(),
        shell_command,
        headless: options.headless,
    })
}

pub const fn launcher_for(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "cmax",
        Engine::Codex => "cdxmax",
        Engine::Opencode => "ocmax",
        Engine::Kimi => "kmax",
        Engine::Grok => "gmax",
    }
}

pub fn default_kickoff(engine: Engine, source_account: &str) -> String {
    let instructions = match engine {
        Engine::Claude => "CLAUDE.md",
        Engine::Codex | Engine::Opencode | Engine::Kimi | Engine::Grok => "AGENTS.md",
    };
    normalize_kickoff(&format!(
        "You are now THE Neomax orchestrator (the previous one rotated off {engine} account {source_account} near its usage limit). Resume immediately with ZERO loss: read this project's {instructions}, then run `neomax ls` to adopt every in-flight/inbox worker, read the program's STATUS.md, and continue the execution loop exactly where it left off."
    ))
}

pub fn render_shell_command(cwd: &Path, launcher: &str, args: &[String]) -> String {
    render_shell_command_for(ShellKind::host(), cwd, launcher, args)
}

pub fn render_shell_command_for(
    shell: ShellKind,
    cwd: &Path,
    launcher: &str,
    args: &[String],
) -> String {
    match shell {
        ShellKind::Posix => render_posix_command(cwd, launcher, args),
        ShellKind::PowerShell => render_powershell_command(cwd, launcher, args),
        ShellKind::Cmd => render_cmd_command(cwd, launcher, args),
    }
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn render_posix_command(cwd: &Path, launcher: &str, args: &[String]) -> String {
    let mut command = format!(
        "cd {} && {}",
        shell_quote(&cwd.to_string_lossy()),
        posix_word(launcher)
    );
    for arg in args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    command
}

fn render_powershell_command(cwd: &Path, launcher: &str, args: &[String]) -> String {
    let mut command = format!(
        "Set-Location -LiteralPath {}; & {}",
        powershell_quote(&cwd.to_string_lossy()),
        powershell_word(launcher)
    );
    for arg in args {
        command.push(' ');
        command.push_str(&powershell_quote(arg));
    }
    command
}

fn render_cmd_command(cwd: &Path, launcher: &str, args: &[String]) -> String {
    let mut command = format!(
        "cd /d {} && {}",
        cmd_quote(&cwd.to_string_lossy()),
        cmd_quote(launcher)
    );
    for arg in args {
        command.push(' ');
        command.push_str(&cmd_quote(arg));
    }
    command
}

fn posix_word(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._/:@+".contains(&byte))
    {
        value.to_owned()
    } else {
        shell_quote(value)
    }
}

fn powershell_word(value: &str) -> String {
    powershell_quote(value)
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn cmd_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    let mut backslashes = 0;
    for character in value.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                push_backslashes(&mut quoted, backslashes * 2 + 1);
                quoted.push('"');
                backslashes = 0;
            }
            '%' => {
                push_backslashes(&mut quoted, backslashes);
                quoted.push_str("%%");
                backslashes = 0;
            }
            character => {
                push_backslashes(&mut quoted, backslashes);
                quoted.push(character);
                backslashes = 0;
            }
        }
    }
    push_backslashes(&mut quoted, backslashes * 2);
    quoted.push('"');
    quoted
}

fn push_backslashes(output: &mut String, count: usize) {
    output.extend(std::iter::repeat_n('\\', count));
}

fn normalize_kickoff(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
