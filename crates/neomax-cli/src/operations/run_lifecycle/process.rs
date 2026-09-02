use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::io::process_group;
use neomax_core::runs::{ProcessProbe, SystemProcessProbe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessTarget {
    Supervisor,
    Worker,
}

pub(crate) trait ProcessControl: ProcessProbe + Send + Sync {
    fn terminate(&self, pid: u32, target: ProcessTarget) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SystemProcessControl;

impl ProcessProbe for SystemProcessControl {
    fn pid_alive(&self, pid: u32) -> bool {
        SystemProcessProbe.pid_alive(pid)
    }

    fn worker_alive(&self, worker_pid: u32, engine: Engine) -> bool {
        SystemProcessProbe.worker_alive(worker_pid, engine)
    }
}

impl ProcessControl for SystemProcessControl {
    fn terminate(&self, pid: u32, target: ProcessTarget) -> Result<()> {
        validate_pid(pid)?;
        if pid == std::process::id() {
            bail!("refusing to terminate the current Neomax process");
        }
        let result = match target {
            ProcessTarget::Supervisor => process_group::terminate_supervisor(pid),
            ProcessTarget::Worker => process_group::terminate_worker(pid),
        };
        if let Err(error) = result {
            bail!(
                "could not terminate {} process {pid}: {error}",
                match target {
                    ProcessTarget::Supervisor => "supervisor",
                    ProcessTarget::Worker => "worker",
                }
            );
        }
        Ok(())
    }
}

pub(crate) fn validate_pid(pid: u32) -> Result<()> {
    if pid <= 1 {
        bail!("refusing unsafe process id {pid}");
    }
    Ok(())
}
