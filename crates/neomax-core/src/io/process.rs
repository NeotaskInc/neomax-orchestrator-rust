use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use super::clock::{Clock, SystemClock};
use super::error::{BoundedIoError, Result};
use super::process_group::{self, ChildContainment, ProcessControl, SystemProcessControl};
use crate::runtime::RuntimeEnvironment;

pub const DEFAULT_PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_TERMINATE_GRACE: Duration = Duration::from_millis(250);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
pub struct ProcessRequest {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: Option<std::path::PathBuf>,
    pub environment: BTreeMap<OsString, OsString>,
    pub remove_environment: BTreeSet<OsString>,
    pub clear_environment: bool,
    pub timeout: Duration,
    pub terminate_grace: Duration,
    pub stdout_limit: usize,
    pub stderr_limit: usize,
    runtime_environment: Option<RuntimeEnvironment>,
}

impl ProcessRequest {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            environment: BTreeMap::new(),
            remove_environment: BTreeSet::new(),
            clear_environment: false,
            timeout: DEFAULT_PROCESS_TIMEOUT,
            terminate_grace: DEFAULT_TERMINATE_GRACE,
            stdout_limit: 128 * 1024,
            stderr_limit: 128 * 1024,
            runtime_environment: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, T>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, cwd: impl Into<std::path::PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }

    pub fn remove_env(mut self, key: impl Into<OsString>) -> Self {
        self.remove_environment.insert(key.into());
        self
    }

    pub fn clear_env(mut self) -> Self {
        self.clear_environment = true;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn terminate_grace(mut self, grace: Duration) -> Self {
        self.terminate_grace = grace;
        self
    }

    pub fn stdout_limit(mut self, limit: usize) -> Self {
        self.stdout_limit = limit;
        self
    }

    pub fn stderr_limit(mut self, limit: usize) -> Self {
        self.stderr_limit = limit;
        self
    }

    pub fn runtime_environment(mut self, environment: RuntimeEnvironment) -> Self {
        self.runtime_environment = Some(environment);
        self
    }

    fn validate(&self) -> Result<()> {
        if self.program.is_empty() {
            return Err(BoundedIoError::InvalidLimit(
                "process program must not be empty".into(),
            ));
        }
        if self.timeout.is_zero() {
            return Err(BoundedIoError::InvalidLimit(
                "process timeout must be greater than zero".into(),
            ));
        }
        if self.stdout_limit == 0 || self.stderr_limit == 0 {
            return Err(BoundedIoError::InvalidLimit(
                "process output limits must be greater than zero".into(),
            ));
        }
        if self.stdout_limit == usize::MAX || self.stderr_limit == usize::MAX {
            return Err(BoundedIoError::InvalidLimit(
                "process output limits must be less than usize::MAX".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    pub status_code: Option<i32>,
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

impl ProcessOutput {
    pub fn strict(self, request: &ProcessRequest) -> Result<Self> {
        if self.timed_out {
            return Err(BoundedIoError::timeout(
                request.program.to_string_lossy(),
                request.timeout,
            ));
        }
        if self.stdout_truncated {
            return Err(BoundedIoError::Truncated {
                operation: format!("{} stdout", request.program.to_string_lossy()),
                limit: request.stdout_limit,
            });
        }
        if self.stderr_truncated {
            return Err(BoundedIoError::Truncated {
                operation: format!("{} stderr", request.program.to_string_lossy()),
                limit: request.stderr_limit,
            });
        }
        if !self.success {
            return Err(BoundedIoError::ProcessFailed {
                program: request.program.to_string_lossy().into_owned(),
                code: self.status_code,
            });
        }
        Ok(self)
    }
}

pub trait ProcessRunner: Send + Sync {
    fn capture(&self, request: &ProcessRequest) -> Result<ProcessOutput>;

    fn execute(&self, request: &ProcessRequest) -> Result<ProcessOutput> {
        request.validate()?;
        self.capture(request)?.strict(request)
    }
}

pub struct LocalProcessRunner {
    clock: Arc<dyn Clock>,
}

impl Default for LocalProcessRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalProcessRunner {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self { clock }
    }
}

impl ProcessRunner for LocalProcessRunner {
    fn capture(&self, request: &ProcessRequest) -> Result<ProcessOutput> {
        request.validate()?;
        let runtime_environment = request
            .runtime_environment
            .clone()
            .unwrap_or_else(RuntimeEnvironment::process);
        let current_dir = request
            .cwd
            .as_deref()
            .unwrap_or_else(|| runtime_environment.current_dir());
        let mut command = runtime_environment
            .process_command(&request.program, &request.args, current_dir)
            .map_err(|error| BoundedIoError::Spawn {
                program: request.program.to_string_lossy().into_owned(),
                source: std::io::Error::other(error.to_string()),
            })?;
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        if request.clear_environment {
            command.env_clear();
        }
        for key in &request.remove_environment {
            command.env_remove(key);
        }
        command.envs(&request.environment);
        if let Some(cwd) = request.cwd.as_ref() {
            command.current_dir(cwd);
        }
        let (mut child, containment) =
            process_group::spawn_managed(&mut command).map_err(|error| BoundedIoError::Spawn {
                program: request.program.to_string_lossy().into_owned(),
                source: std::io::Error::other(error.to_string()),
            })?;
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = terminate_and_reap(&mut child, &containment, request.terminate_grace);
                return Err(BoundedIoError::MissingPipe {
                    operation: "stdout capture".into(),
                });
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                let _ = terminate_and_reap(&mut child, &containment, request.terminate_grace);
                return Err(BoundedIoError::MissingPipe {
                    operation: "stderr capture".into(),
                });
            }
        };
        let stdout_signal = Arc::new(AtomicBool::new(false));
        let stderr_signal = Arc::new(AtomicBool::new(false));
        let stdout_thread = spawn_reader(stdout, request.stdout_limit, Arc::clone(&stdout_signal));
        let stderr_thread = spawn_reader(stderr, request.stderr_limit, Arc::clone(&stderr_signal));
        let started = self.clock.now();
        let mut timed_out = false;
        let mut interrupted = false;
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|source| BoundedIoError::io(request.program.to_string_lossy(), source))?
            {
                terminate_residual(&containment, request.terminate_grace);
                break status;
            }
            if stdout_signal.load(Ordering::Acquire) || stderr_signal.load(Ordering::Acquire) {
                interrupted = true;
                break terminate_and_reap(&mut child, &containment, request.terminate_grace)
                    .map_err(|source| {
                        BoundedIoError::io(request.program.to_string_lossy(), source)
                    })?;
            }
            if self.clock.now().duration_since(started) >= request.timeout {
                timed_out = true;
                break terminate_and_reap(&mut child, &containment, request.terminate_grace)
                    .map_err(|source| {
                        BoundedIoError::io(request.program.to_string_lossy(), source)
                    })?;
            }
            self.clock.sleep(POLL_INTERVAL);
        };
        let stdout_result = stdout_thread
            .join()
            .map_err(|_| BoundedIoError::MissingPipe {
                operation: "stdout capture".into(),
            })?;
        let stderr_result = stderr_thread
            .join()
            .map_err(|_| BoundedIoError::MissingPipe {
                operation: "stderr capture".into(),
            })?;
        let stdout_truncated =
            stdout_result.truncated || interrupted && stdout_signal.load(Ordering::Acquire);
        let stderr_truncated =
            stderr_result.truncated || interrupted && stderr_signal.load(Ordering::Acquire);
        if let Some(error) = stdout_result.error.or(stderr_result.error) {
            if !timed_out && !stdout_truncated && !stderr_truncated {
                return Err(error);
            }
        }
        Ok(ProcessOutput {
            status_code: status.code(),
            success: status.success() && !timed_out && !stdout_truncated && !stderr_truncated,
            stdout: stdout_result.bytes,
            stderr: stderr_result.bytes,
            timed_out,
            stdout_truncated,
            stderr_truncated,
        })
    }
}

struct ReaderResult {
    bytes: Vec<u8>,
    truncated: bool,
    error: Option<BoundedIoError>,
}

fn spawn_reader<R: std::io::Read + Send + 'static>(
    mut reader: R,
    limit: usize,
    signal: Arc<AtomicBool>,
) -> thread::JoinHandle<ReaderResult> {
    thread::spawn(move || {
        let mut bytes = Vec::with_capacity(limit.min(16 * 1024));
        let mut buffer = vec![0u8; 16 * 1024];
        loop {
            let remaining = limit.saturating_sub(bytes.len());
            let request = remaining.saturating_add(1).min(buffer.len());
            let read = match reader.read(&mut buffer[..request]) {
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return ReaderResult {
                        bytes,
                        truncated: false,
                        error: Some(BoundedIoError::io("process output", error)),
                    };
                }
            };
            if read == 0 {
                return ReaderResult {
                    bytes,
                    truncated: false,
                    error: None,
                };
            }
            if read > remaining {
                signal.store(true, Ordering::Release);
                return ReaderResult {
                    bytes,
                    truncated: true,
                    error: None,
                };
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
    })
}

fn terminate_and_reap(
    child: &mut Child,
    containment: &ChildContainment,
    grace: Duration,
) -> std::io::Result<ExitStatus> {
    SystemProcessControl.terminate(child, containment, grace);
    if let Some(status) = child.try_wait()? {
        return Ok(status);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "process tree did not exit after forced termination",
    ))
}

fn terminate_residual(containment: &ChildContainment, grace: Duration) {
    SystemProcessControl.terminate_residual(containment, grace);
}
