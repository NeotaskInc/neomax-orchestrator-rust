use std::process::{Child, Command};
use std::time::Duration;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildContainment {
    pid: u32,
}

pub(super) fn configure(_command: &mut Command) {}

pub(super) fn configure_detached(_command: &mut Command) {}

pub(super) fn spawn_managed(command: &mut Command) -> Result<(Child, ChildContainment)> {
    configure(command);
    let mut child = command.spawn()?;
    match attach(child.id()) {
        Ok(containment) => Ok((child, containment)),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
}

pub(super) fn attach(pid: u32) -> Result<ChildContainment> {
    Ok(ChildContainment { pid })
}

pub(super) fn terminate(child: &mut Child, containment: &ChildContainment, _grace: Duration) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = containment.pid;
}

pub(super) fn terminate_residual(_containment: &ChildContainment, _grace: Duration) {}

pub(super) fn terminate_detached(child: &mut Child, _grace: Duration) -> std::io::Result<()> {
    let _ = child.kill();
    let _ = child.wait()?;
    Ok(())
}

pub(super) fn terminate_process_group(pid: u32) -> std::io::Result<()> {
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()?;
    if status.success() {
        return Ok(());
    }
    Err(std::io::Error::other(format!(
        "kill command failed for process group {pid}"
    )))
}
