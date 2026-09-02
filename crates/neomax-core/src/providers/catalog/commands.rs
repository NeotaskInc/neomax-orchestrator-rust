use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use crate::io::{LocalProcessRunner, ProcessRequest, ProcessRunner};
use crate::runtime::{RuntimeEnvironment, RuntimePlatform};
use crate::Result;

use super::environment::Environment;
use super::specs::spec;
use super::types::{BinaryStatus, ProviderSpec};
use crate::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryCommand {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub safe_environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub timed_out: bool,
    pub truncated: bool,
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, command: &DiscoveryCommand) -> Result<CommandOutput>;
}

pub const DEFAULT_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
pub const DEFAULT_MAX_STDOUT_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalCommandRunner {
    timeout: Duration,
    max_stdout_bytes: usize,
}

impl Default for LocalCommandRunner {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_DISCOVERY_TIMEOUT,
            max_stdout_bytes: DEFAULT_MAX_STDOUT_BYTES,
        }
    }
}

impl LocalCommandRunner {
    pub fn new(timeout: Duration, max_stdout_bytes: usize) -> Self {
        Self {
            timeout,
            max_stdout_bytes: max_stdout_bytes.clamp(1, usize::MAX - 1),
        }
    }

    pub fn timeout(self) -> Duration {
        self.timeout
    }

    pub fn max_stdout_bytes(self) -> usize {
        self.max_stdout_bytes
    }
}

impl CommandRunner for LocalCommandRunner {
    fn run(&self, command: &DiscoveryCommand) -> Result<CommandOutput> {
        let cwd = match command.cwd.as_ref() {
            Some(cwd) => cwd.clone(),
            None => std::env::current_dir()?,
        };
        let runtime = RuntimeEnvironment::fixture(
            RuntimePlatform::current(),
            command
                .safe_environment
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
            cwd,
        );
        let arguments = command
            .args
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        let mut request = ProcessRequest::new(&command.program)
            .args(arguments)
            .clear_env()
            .runtime_environment(runtime)
            .timeout(self.timeout)
            .stdout_limit(self.max_stdout_bytes)
            .stderr_limit(self.max_stdout_bytes);
        if let Some(cwd) = command.cwd.as_ref() {
            request = request.cwd(cwd.clone());
        }
        for (key, value) in &command.safe_environment {
            request = request.env(key.clone(), value.clone());
        }
        let output = LocalProcessRunner::default().capture(&request)?;
        Ok(CommandOutput {
            success: output.success
                && !output.timed_out
                && !output.stdout_truncated
                && !output.stderr_truncated,
            stdout: output.stdout,
            timed_out: output.timed_out,
            truncated: output.stdout_truncated || output.stderr_truncated,
        })
    }
}

pub fn binary_status(
    engine: Engine,
    environment: &dyn Environment,
    runner: &dyn CommandRunner,
) -> BinaryStatus {
    let provider = spec(engine);
    let program = environment
        .value(&provider.binary_env)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| provider.default_binary.clone());
    let command = DiscoveryCommand {
        program: program.clone(),
        args: vec!["--version".into()],
        cwd: Some(environment.current_dir()),
        safe_environment: safe_environment(environment, &provider),
    };
    match runner.run(&command) {
        Ok(output) if output.success && !output.timed_out && !output.truncated => BinaryStatus {
            program,
            available: true,
            version: first_line(&output.stdout),
        },
        _ => BinaryStatus {
            program,
            available: false,
            version: None,
        },
    }
}

pub fn model_ids(
    engine: Engine,
    environment: &dyn Environment,
    runner: &dyn CommandRunner,
) -> Vec<String> {
    let provider = spec(engine);
    if provider.model_args.is_empty() {
        return Vec::new();
    }
    let program = environment
        .value(&provider.binary_env)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| provider.default_binary.clone());
    let command = DiscoveryCommand {
        program,
        args: provider.model_args.clone(),
        cwd: Some(environment.current_dir()),
        safe_environment: safe_environment(environment, &provider),
    };
    let Ok(output) = runner.run(&command) else {
        return Vec::new();
    };
    if !output.success || output.timed_out || output.truncated {
        return Vec::new();
    }
    parse_model_ids(&output.stdout, engine)
}

fn safe_environment(
    environment: &dyn Environment,
    provider: &ProviderSpec,
) -> BTreeMap<String, String> {
    environment.safe_child_environment(Some(&provider.config_env))
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_model_ids(bytes: &[u8], engine: Engine) -> Vec<String> {
    if engine == Engine::Kimi {
        return parse_kimi_model_ids(bytes);
    }
    let text = String::from_utf8_lossy(bytes);
    let mut ids = Vec::new();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
        collect_json_models(&value, engine, &mut ids);
    } else {
        for line in text.lines() {
            let candidate = line
                .trim()
                .trim_start_matches(['-', '*', '•', ' '])
                .split_whitespace()
                .next()
                .unwrap_or_default();
            if looks_like_model(candidate, engine) {
                ids.push(candidate.to_string());
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

fn parse_kimi_model_ids(bytes: &[u8]) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Vec::new();
    };
    let mut ids = Vec::new();

    if let Some(models) = value.get("models") {
        collect_kimi_models(models, &mut ids);
    }
    if let Some(providers) = value.get("providers") {
        collect_kimi_provider_models(providers, &mut ids);
    }

    ids.sort();
    ids.dedup();
    ids
}

fn collect_kimi_models(value: &serde_json::Value, output: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(entries) => {
            for (alias, definition) in entries {
                if looks_like_model(alias, Engine::Kimi) {
                    output.push(alias.clone());
                }
                collect_kimi_model_definition(definition, output);
            }
        }
        serde_json::Value::Array(entries) => {
            entries
                .iter()
                .for_each(|entry| collect_kimi_model_definition(entry, output));
        }
        _ => {}
    }
}

fn collect_kimi_provider_models(value: &serde_json::Value, output: &mut Vec<String>) {
    let serde_json::Value::Object(providers) = value else {
        return;
    };
    for definition in providers.values() {
        if let serde_json::Value::Object(definition) = definition {
            if let Some(models) = definition.get("models") {
                collect_kimi_models(models, output);
            }
        }
    }
}

fn collect_kimi_model_definition(value: &serde_json::Value, output: &mut Vec<String>) {
    match value {
        serde_json::Value::String(value) if looks_like_model(value, Engine::Kimi) => {
            output.push(value.clone());
        }
        serde_json::Value::Object(entries) => {
            for key in ["id", "model", "name", "alias"] {
                if let Some(serde_json::Value::String(value)) = entries.get(key) {
                    if looks_like_model(value, Engine::Kimi) {
                        output.push(value.clone());
                    }
                }
            }
        }
        serde_json::Value::Array(entries) => entries
            .iter()
            .for_each(|entry| collect_kimi_model_definition(entry, output)),
        _ => {}
    }
}

fn collect_json_models(value: &serde_json::Value, engine: Engine, output: &mut Vec<String>) {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .for_each(|value| collect_json_models(value, engine, output)),
        serde_json::Value::Object(values) => {
            for key in ["id", "model", "name"] {
                if let Some(serde_json::Value::String(value)) = values.get(key) {
                    if looks_like_model(value, engine) {
                        output.push(value.clone());
                    }
                }
            }
            values
                .values()
                .for_each(|value| collect_json_models(value, engine, output));
        }
        _ => {}
    }
}

fn looks_like_model(value: &str, engine: Engine) -> bool {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_whitespace) {
        return false;
    }
    match engine {
        Engine::Opencode => value
            .split_once('/')
            .is_some_and(|(p, m)| !p.is_empty() && !m.is_empty()),
        _ => value
            .chars()
            .any(|character| character.is_ascii_alphanumeric()),
    }
}

impl From<OsString> for DiscoveryCommand {
    fn from(program: OsString) -> Self {
        Self {
            program: program.to_string_lossy().into_owned(),
            args: Vec::new(),
            cwd: None,
            safe_environment: BTreeMap::new(),
        }
    }
}
