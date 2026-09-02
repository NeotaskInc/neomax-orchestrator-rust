use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Duration;

use crate::io::{LocalProcessRunner, ProcessRequest, ProcessRunner};
use crate::providers::scrub_provider_process_request;
use crate::{Error, Result};

pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_COMMAND_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_CONFIGURED_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_CONFIGURED_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitProcessConfig {
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

impl GitProcessConfig {
    pub fn new(timeout: Duration, max_output_bytes: usize) -> Self {
        Self {
            timeout: if timeout > MAX_CONFIGURED_COMMAND_TIMEOUT {
                MAX_CONFIGURED_COMMAND_TIMEOUT
            } else {
                timeout
            },
            max_output_bytes: if max_output_bytes == 0 {
                1
            } else if max_output_bytes > MAX_CONFIGURED_OUTPUT_BYTES {
                MAX_CONFIGURED_OUTPUT_BYTES
            } else {
                max_output_bytes
            },
        }
    }
}

impl Default for GitProcessConfig {
    fn default() -> Self {
        Self::new(DEFAULT_COMMAND_TIMEOUT, MAX_COMMAND_OUTPUT_BYTES)
    }
}

#[derive(Debug, Clone)]
pub struct GitOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl GitOutput {
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().into()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).trim().into()
    }
}

pub fn invoke(cwd: &Path, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Result<GitOutput> {
    invoke_with_config(cwd, args, GitProcessConfig::default())
}

pub fn invoke_with_config(
    cwd: &Path,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    config: GitProcessConfig,
) -> Result<GitOutput> {
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<OsString>>();
    let request = ProcessRequest::new("git")
        .args(args)
        .cwd(cwd)
        .timeout(config.timeout)
        .stdout_limit(config.max_output_bytes)
        .stderr_limit(config.max_output_bytes);
    let request = scrub_provider_process_request(request);
    let output = LocalProcessRunner::default().capture(&request)?;
    if output.timed_out {
        return Err(Error::Message(format!(
            "git command timed out after {} milliseconds",
            config.timeout.as_millis()
        )));
    }
    if output.stdout_truncated || output.stderr_truncated {
        return Err(Error::Message(format!(
            "git command output exceeded {} bytes",
            config.max_output_bytes
        )));
    }
    Ok(GitOutput {
        success: output.success,
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

pub fn checked_text(
    cwd: &Path,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<String> {
    let output = invoke(cwd, args)?;
    if !output.success {
        return Err(Error::Message(output.stderr_text()));
    }
    Ok(output.stdout_text())
}
