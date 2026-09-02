use std::path::Path;

use crate::io::{LocalProcessRunner, ProcessRequest, ProcessRunner};
use crate::providers::scrub_provider_process_request;
use crate::{Error, Result};

use super::super::inspection::GitCommandOutput;

pub const GH_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub const MAX_GH_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhCommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait GhCommandRunner: Send + Sync {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<GhCommandOutput>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessGhRunner;

impl GhCommandRunner for ProcessGhRunner {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<GhCommandOutput> {
        let request = ProcessRequest::new("gh")
            .args(args.iter().cloned())
            .cwd(cwd)
            .timeout(GH_COMMAND_TIMEOUT)
            .stdout_limit(MAX_GH_OUTPUT_BYTES)
            .stderr_limit(MAX_GH_OUTPUT_BYTES);
        let request = scrub_provider_process_request(request);
        let output = LocalProcessRunner::default().capture(&request)?;
        if output.timed_out {
            return Err(Error::Message(format!(
                "gh command timed out after {} milliseconds",
                GH_COMMAND_TIMEOUT.as_millis()
            )));
        }
        if output.stdout_truncated || output.stderr_truncated {
            return Err(Error::Message(format!(
                "gh command output exceeded {MAX_GH_OUTPUT_BYTES} bytes"
            )));
        }
        Ok(GhCommandOutput {
            success: output.success,
            stdout: String::from_utf8_lossy(&output.stdout).trim().into(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
        })
    }
}

pub fn gh_failure(output: &GhCommandOutput, operation: &str) -> Error {
    let detail = if output.stderr.is_empty() {
        output.stdout.as_str()
    } else {
        output.stderr.as_str()
    };
    Error::Message(format!("gh {operation} failed: {}", truncate(detail)))
}

fn truncate(value: &str) -> &str {
    let end = value
        .char_indices()
        .nth(600)
        .map_or(value.len(), |(index, _)| index);
    &value[..end]
}

pub fn git_failure(output: &GitCommandOutput, operation: &str) -> Error {
    let detail = if output.stderr.is_empty() {
        output.stdout.as_str()
    } else {
        output.stderr.as_str()
    };
    Error::Message(format!("git {operation} failed: {}", truncate(detail)))
}
