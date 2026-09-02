use std::time::Duration;

use crate::Engine;
use crate::io::{LocalProcessRunner, ProcessOutput, ProcessRequest, ProcessRunner};
use crate::providers::scrub_provider_process_request;

use super::{RunRecord, RunStatus};

const PROCESS_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const PROCESS_PROBE_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProbeState {
    #[default]
    Unknown,
    Alive,
    Dead,
}

impl ProbeState {
    pub const fn is_alive(self) -> bool {
        matches!(self, Self::Alive)
    }

    pub const fn is_dead(self) -> bool {
        matches!(self, Self::Dead)
    }
}

pub trait ProcessProbe: Send + Sync {
    fn pid_alive(&self, pid: u32) -> bool;
    fn worker_alive(&self, worker_pid: u32, engine: Engine) -> bool;

    fn pid_state(&self, pid: u32) -> ProbeState {
        if self.pid_alive(pid) {
            ProbeState::Alive
        } else {
            ProbeState::Dead
        }
    }

    fn worker_state(&self, worker_pid: u32, engine: Engine) -> ProbeState {
        if self.worker_alive(worker_pid, engine) {
            ProbeState::Alive
        } else {
            ProbeState::Dead
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemProcessProbe;

impl ProcessProbe for SystemProcessProbe {
    fn pid_alive(&self, pid: u32) -> bool {
        self.pid_state(pid).is_alive()
    }

    fn worker_alive(&self, worker_pid: u32, engine: Engine) -> bool {
        self.worker_state(worker_pid, engine).is_alive()
    }

    fn pid_state(&self, pid: u32) -> ProbeState {
        pid_state(pid)
    }

    fn worker_state(&self, worker_pid: u32, engine: Engine) -> ProbeState {
        match self.pid_state(worker_pid) {
            ProbeState::Alive => process_command(worker_pid, engine),
            ProbeState::Dead => group_has_engine(worker_pid, engine),
            ProbeState::Unknown => ProbeState::Unknown,
        }
    }
}

pub fn effective_status(run: &RunRecord, probe: &(impl ProcessProbe + ?Sized)) -> RunStatus {
    if run.status != RunStatus::Running {
        return run.status;
    }
    match run
        .supervisor_pid
        .map_or(ProbeState::Dead, |pid| probe.pid_state(pid))
    {
        ProbeState::Alive => return RunStatus::Running,
        ProbeState::Unknown => return RunStatus::Unknown,
        ProbeState::Dead => {}
    }
    match worker_state(run, probe) {
        ProbeState::Alive => RunStatus::Orphaned,
        ProbeState::Dead => RunStatus::Interrupted,
        ProbeState::Unknown => RunStatus::Unknown,
    }
}

pub fn worker_alive(run: &RunRecord, probe: &(impl ProcessProbe + ?Sized)) -> bool {
    worker_state(run, probe).is_alive()
}

pub fn worker_state(run: &RunRecord, probe: &(impl ProcessProbe + ?Sized)) -> ProbeState {
    run.worker_pid
        .map_or(ProbeState::Dead, |pid| probe.worker_state(pid, run.engine))
}

pub fn in_inbox(run: &RunRecord, probe: &(impl ProcessProbe + ?Sized)) -> bool {
    effective_status(run, probe).is_terminal() && !run.is_acknowledged()
}

#[cfg(unix)]
fn pid_state(pid: u32) -> ProbeState {
    if pid == 0 {
        return ProbeState::Unknown;
    }
    let Ok(pid) = i32::try_from(pid) else {
        return ProbeState::Unknown;
    };
    // SAFETY: signal zero only probes a validated process id.
    let result = unsafe { libc::kill(pid, 0) };
    if result == 0 {
        return ProbeState::Alive;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::EPERM) => ProbeState::Alive,
        Some(libc::ESRCH) => ProbeState::Dead,
        _ => ProbeState::Unknown,
    }
}

#[cfg(not(unix))]
fn pid_state(pid: u32) -> ProbeState {
    if pid == 0 {
        return ProbeState::Unknown;
    }
    let filter = format!("PID eq {pid}");
    let Some(output) = bounded_process("tasklist", &["/FI", &filter, "/NH"]) else {
        return ProbeState::Unknown;
    };
    if !output.success {
        return ProbeState::Unknown;
    }
    if String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        line.split_whitespace()
            .any(|field| field == pid.to_string())
    }) {
        ProbeState::Alive
    } else {
        ProbeState::Dead
    }
}

#[cfg(unix)]
fn process_command(pid: u32, engine: Engine) -> ProbeState {
    let pid = pid.to_string();
    classify_process_output(
        bounded_process("ps", &["-o", "command=", "-p", &pid]).as_ref(),
        engine,
    )
}

#[cfg(not(unix))]
fn process_command(pid: u32, engine: Engine) -> ProbeState {
    let filter = format!("PID eq {pid}");
    classify_process_output(
        bounded_process("tasklist", &["/FI", &filter, "/FO", "CSV", "/NH"]).as_ref(),
        engine,
    )
}

#[cfg(unix)]
fn group_has_engine(process_group: u32, engine: Engine) -> ProbeState {
    classify_group_output(
        bounded_process("ps", &["-axo", "pgid=,command="]).as_ref(),
        process_group,
        engine,
    )
}

fn classify_process_output(output: Option<&ProcessOutput>, engine: Engine) -> ProbeState {
    let Some(output) = output else {
        return ProbeState::Unknown;
    };
    if !output.success {
        return ProbeState::Unknown;
    }
    let command = String::from_utf8_lossy(&output.stdout);
    if command.trim().is_empty() || command.contains("INFO:") {
        ProbeState::Unknown
    } else if command_matches_engine(&command, engine) {
        ProbeState::Alive
    } else {
        ProbeState::Dead
    }
}

#[cfg(unix)]
fn classify_group_output(
    output: Option<&ProcessOutput>,
    process_group: u32,
    engine: Engine,
) -> ProbeState {
    let Some(output) = output else {
        return ProbeState::Unknown;
    };
    if !output.success {
        return ProbeState::Unknown;
    }
    let matching = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().split_once(char::is_whitespace))
        .any(|(group, command)| {
            group.parse::<u32>().ok() == Some(process_group)
                && command_matches_engine(command, engine)
        });
    if matching {
        ProbeState::Alive
    } else {
        ProbeState::Dead
    }
}

fn bounded_process(program: &str, args: &[&str]) -> Option<ProcessOutput> {
    let request = ProcessRequest::new(program)
        .args(args.iter().copied())
        .timeout(PROCESS_PROBE_TIMEOUT)
        .stdout_limit(PROCESS_PROBE_OUTPUT_BYTES)
        .stderr_limit(PROCESS_PROBE_OUTPUT_BYTES);
    let request = scrub_provider_process_request(request);
    LocalProcessRunner::default().capture(&request).ok()
}

#[cfg(not(unix))]
fn group_has_engine(_process_group: u32, _engine: Engine) -> ProbeState {
    ProbeState::Unknown
}

fn command_matches_engine(command: &str, engine: Engine) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains(engine.as_str())
        || ["claude", "codex", "opencode", "kimi", "grok"]
            .iter()
            .any(|name| command.contains(name))
}

#[cfg(test)]
#[path = "liveness_tests/mod.rs"]
mod tests;
