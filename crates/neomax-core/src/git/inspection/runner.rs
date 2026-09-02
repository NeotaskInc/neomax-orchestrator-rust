use std::path::Path;

use crate::{git, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl GitCommandOutput {
    pub fn stdout_text(&self) -> String {
        self.stdout.clone()
    }

    pub fn stderr_text(&self) -> String {
        self.stderr.clone()
    }
}

pub trait GitCommandRunner {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<GitCommandOutput>;
}

pub fn require_success(output: GitCommandOutput, operation: &str) -> Result<GitCommandOutput> {
    if output.success {
        return Ok(output);
    }
    if output.stderr.is_empty() {
        Err(Error::Message(format!("{operation} failed")))
    } else {
        Err(Error::Message(format!(
            "{operation} failed: {}",
            output.stderr
        )))
    }
}

impl<R: GitCommandRunner + ?Sized> GitCommandRunner for &R {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<GitCommandOutput> {
        (*self).run(cwd, args)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessGitRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfiguredGitRunner {
    config: git::GitProcessConfig,
}

impl ConfiguredGitRunner {
    pub const fn new(config: git::GitProcessConfig) -> Self {
        Self { config }
    }

    pub const fn config(self) -> git::GitProcessConfig {
        self.config
    }
}

impl Default for ConfiguredGitRunner {
    fn default() -> Self {
        Self::new(git::GitProcessConfig::default())
    }
}

impl GitCommandRunner for ProcessGitRunner {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<GitCommandOutput> {
        let output = git::invoke(cwd, args.iter().map(String::as_str))?;
        Ok(command_output(output))
    }
}

impl GitCommandRunner for ConfiguredGitRunner {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<GitCommandOutput> {
        let output = git::invoke_with_config(cwd, args.iter().map(String::as_str), self.config)?;
        Ok(command_output(output))
    }
}

fn command_output(output: git::GitOutput) -> GitCommandOutput {
    GitCommandOutput {
        success: output.success,
        stdout: output.stdout_text(),
        stderr: output.stderr_text(),
    }
}
