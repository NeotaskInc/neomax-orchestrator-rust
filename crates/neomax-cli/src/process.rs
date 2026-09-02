use std::process::{Child, Command};

use anyhow::Result;
use neomax_core::io::process_group;

pub(crate) fn spawn_detached(command: &mut Command) -> Result<Child> {
    Ok(process_group::spawn_detached(command)?)
}

pub(crate) fn terminate_detached(child: &mut Child) -> Result<()> {
    Ok(process_group::terminate_detached(child)?)
}

pub(crate) fn terminate_worker(pid: u32) -> Result<()> {
    process_group::terminate_worker(pid)
        .map_err(|error| anyhow::anyhow!("could not terminate worker process {pid}: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};

    use super::spawn_detached;

    #[cfg(unix)]
    use super::terminate_detached;

    #[cfg(unix)]
    #[test]
    fn detached_child_is_a_new_session_and_process_group() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "sleep 2"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = spawn_detached(&mut command).expect("spawn detached child");
        let child_pid = libc::pid_t::try_from(child.id()).expect("child PID fits pid_t");
        let session_id = unsafe { libc::getsid(child_pid) };
        assert_eq!(
            session_id, child_pid,
            "detached child must lead its own session"
        );
        let process_group_id = unsafe { libc::getpgid(child_pid) };
        assert_eq!(
            process_group_id, child_pid,
            "detached child must lead its own process group"
        );
        terminate_detached(&mut child).expect("terminate detached child");
    }

    #[cfg(windows)]
    #[test]
    fn detached_child_runs_with_windows_detachment_flags() {
        let mut command = Command::new("cmd.exe");
        command
            .args(["/C", "exit", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = spawn_detached(&mut command).expect("spawn detached child");
        assert!(child.wait().expect("wait for detached child").success());
    }
}
