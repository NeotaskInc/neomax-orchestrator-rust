use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use neomax_core::io::{LocalProcessRunner, ProcessRequest, ProcessRunner};
use neomax_core::providers::{
    is_secret_environment_key, scrub_provider_environment, scrub_provider_process_request,
};
use neomax_core::runtime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessInvocation {
    pub(crate) program: OsString,
    pub(crate) args: Vec<OsString>,
    pub(crate) cwd: PathBuf,
    pub(crate) environment: BTreeMap<OsString, OsString>,
    pub(crate) remove_environment: BTreeSet<OsString>,
    pub(crate) interactive: bool,
}

impl ProcessInvocation {
    pub(crate) fn new(program: impl Into<OsString>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            environment: BTreeMap::new(),
            remove_environment: BTreeSet::new(),
            interactive: false,
        }
    }

    pub(crate) fn arg(mut self, value: impl Into<OsString>) -> Self {
        self.args.push(value.into());
        self
    }

    pub(crate) fn args<I, T>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        self.args.extend(values.into_iter().map(Into::into));
        self
    }

    pub(crate) fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }

    pub(crate) fn remove_env(mut self, key: impl Into<OsString>) -> Self {
        self.remove_environment.insert(key.into());
        self
    }

    pub(crate) fn interactive(mut self) -> Self {
        self.interactive = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessOutcome {
    pub(crate) status_code: Option<i32>,
    pub(crate) success: bool,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

pub(crate) trait ProcessPort: Send + Sync {
    fn invoke(&self, request: &ProcessInvocation) -> Result<ProcessOutcome>;
}

pub(crate) struct LocalProcessPort;

impl ProcessPort for LocalProcessPort {
    fn invoke(&self, request: &ProcessInvocation) -> Result<ProcessOutcome> {
        if request.interactive {
            let mut command =
                runtime::process_command(&request.program, &request.args, &request.cwd)?;
            command.current_dir(&request.cwd);
            scrub_provider_environment(&mut command);
            for key in &request.remove_environment {
                command.env_remove(key);
            }
            for (key, value) in &request.environment {
                if !is_secret_environment_key(&key.to_string_lossy()) {
                    command.env(key, value);
                }
            }
            command
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
            let status = command.status().with_context(|| {
                format!("could not start {}", request.program.to_string_lossy())
            })?;
            return Ok(ProcessOutcome {
                status_code: status.code(),
                success: status.success(),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }

        let mut process = ProcessRequest::new(request.program.clone())
            .args(request.args.clone())
            .cwd(request.cwd.clone())
            .stdout_limit(128 * 1024)
            .stderr_limit(128 * 1024);
        process = scrub_provider_process_request(process);
        for (key, value) in &request.environment {
            if !is_secret_environment_key(&key.to_string_lossy()) {
                process = process.env(key.clone(), value.clone());
            }
        }
        for key in &request.remove_environment {
            process = process.remove_env(key.clone());
        }
        let output = LocalProcessRunner::default()
            .capture(&process)
            .map_err(|error| anyhow::anyhow!(error))?;
        Ok(ProcessOutcome {
            status_code: output.status_code,
            success: output.success,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invocation_builder_keeps_environment_isolated() {
        let invocation = ProcessInvocation::new("fixture", "/workspace")
            .arg("run")
            .env("CODEX_HOME", "/profiles/2")
            .remove_env("OPENAI_API_KEY")
            .interactive();
        assert_eq!(invocation.args, [OsString::from("run")]);
        assert_eq!(
            invocation.environment.get(&OsString::from("CODEX_HOME")),
            Some(&OsString::from("/profiles/2"))
        );
        assert!(
            invocation
                .remove_environment
                .contains(&OsString::from("OPENAI_API_KEY"))
        );
        assert!(invocation.interactive);
    }
}
