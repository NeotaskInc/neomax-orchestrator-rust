use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use neomax_core::providers::scrub_provider_environment;
use neomax_core::runtime;

#[cfg(windows)]
const PORTAL_BINARY_FILENAME: &str = "neomax-portal.exe";
#[cfg(not(windows))]
const PORTAL_BINARY: &str = "neomax-portal";
#[cfg(not(windows))]
const PORTAL_BINARY_FILENAME: &str = PORTAL_BINARY;
const PORTAL_BINARY_ENV: &str = "NEOMAX_PORTAL_BIN";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortalInvocation {
    pub(crate) executable: PathBuf,
    pub(crate) args: Vec<String>,
}

impl PortalInvocation {
    fn new(executable: PathBuf, args: &[String]) -> Self {
        Self {
            executable,
            args: args.to_vec(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PortalExit {
    code: Option<i32>,
    signal: Option<i32>,
}

impl PortalExit {
    #[cfg(test)]
    fn success() -> Self {
        Self {
            code: Some(0),
            signal: None,
        }
    }

    fn from_status(status: std::process::ExitStatus) -> Self {
        #[cfg(unix)]
        let signal = std::os::unix::process::ExitStatusExt::signal(&status);
        #[cfg(not(unix))]
        let signal = None;
        Self {
            code: status.code(),
            signal,
        }
    }

    fn is_success(self) -> bool {
        self.code == Some(0) && self.signal.is_none()
    }

    pub(crate) fn code(self) -> Option<i32> {
        self.code
    }

    pub(crate) fn signal(self) -> Option<i32> {
        self.signal
    }
}

pub(crate) trait PortalExecutor: Send + Sync {
    fn invoke(&self, invocation: &PortalInvocation) -> Result<PortalExit>;
}

struct LocalPortalExecutor;

impl PortalExecutor for LocalPortalExecutor {
    fn invoke(&self, invocation: &PortalInvocation) -> Result<PortalExit> {
        let current_dir = std::env::current_dir().unwrap_or_default();
        let mut command =
            runtime::process_command(&invocation.executable, &invocation.args, &current_dir)?;
        scrub_provider_environment(&mut command);
        command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let status = command.status().with_context(|| {
            format!(
                "could not start portal executable {}",
                invocation.executable.display()
            )
        })?;
        Ok(PortalExit::from_status(status))
    }
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    let current_executable =
        std::env::current_exe().context("could not locate neomax executable")?;
    run_from(&current_executable, args)
}

pub(crate) fn run_from(current_executable: &Path, args: &[String]) -> Result<()> {
    let executor = LocalPortalExecutor;
    run_with_executor(current_executable, args, &executor)
}

pub(crate) fn run_with_executor(
    current_executable: &Path,
    args: &[String],
    executor: &dyn PortalExecutor,
) -> Result<()> {
    let override_path = std::env::var_os(PORTAL_BINARY_ENV).map(PathBuf::from);
    run_with_executor_path(current_executable, args, executor, override_path.as_deref())
}

fn run_with_executor_path(
    current_executable: &Path,
    args: &[String],
    executor: &dyn PortalExecutor,
    override_path: Option<&Path>,
) -> Result<()> {
    let executable = portal_executable_from_override(current_executable, override_path)?;
    let invocation = PortalInvocation::new(executable, args);
    let exit = executor.invoke(&invocation)?;
    if exit.is_success() {
        return Ok(());
    }
    if let Some(signal) = exit.signal() {
        bail!("neomax-portal terminated by signal {signal}");
    }
    bail!(
        "neomax-portal exited with status {}",
        exit.code()
            .map_or_else(|| "unknown".into(), |code| code.to_string())
    );
}

fn portal_executable_from_override(
    current_executable: &Path,
    override_path: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(path) = override_path {
        if path.as_os_str().is_empty() {
            bail!("{PORTAL_BINARY_ENV} must not be empty");
        }
        return validated_override(path);
    }
    sibling_executable(current_executable)
}

fn validated_override(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!(
            "{PORTAL_BINARY_ENV} must be an absolute executable path: {}",
            path.display()
        );
    }
    let metadata = fs::metadata(path).with_context(|| {
        format!(
            "{PORTAL_BINARY_ENV} does not identify an installed portal executable: {}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        bail!(
            "{PORTAL_BINARY_ENV} is not a regular file: {}",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("{PORTAL_BINARY_ENV} is not executable: {}", path.display());
        }
    }
    Ok(path.to_owned())
}

pub(crate) fn sibling_executable(current_executable: &Path) -> Result<PathBuf> {
    let parent = current_executable
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| anyhow::anyhow!("neomax executable has no containing directory"))?;
    let candidate = parent.join(PORTAL_BINARY_FILENAME);
    let metadata = fs::symlink_metadata(&candidate).with_context(|| {
        format!(
            "installed portal executable is missing beside {}",
            current_executable.display()
        )
    })?;
    if !metadata.file_type().is_file() {
        bail!(
            "installed portal executable is not a regular file: {}",
            candidate.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!(
                "installed portal executable is not executable: {}",
                candidate.display()
            );
        }
    }
    Ok(candidate)
}

#[cfg(test)]
fn run_with_executor_without_environment(
    current_executable: &Path,
    args: &[String],
    executor: &dyn PortalExecutor,
    override_path: Option<&Path>,
) -> Result<()> {
    run_with_executor_path(current_executable, args, executor, override_path)
}

#[cfg(test)]
#[path = "portal/tests.rs"]
mod tests;
