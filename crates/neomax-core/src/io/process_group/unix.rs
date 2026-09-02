use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildContainment {
    process_group: u32,
}

pub(super) fn configure(command: &mut Command) {
    command.process_group(0);
}

pub(super) fn configure_detached(command: &mut Command) {
    // SAFETY: the hook only calls setsid, which is async-signal-safe and does
    // not access Rust state between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}

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
    Ok(ChildContainment { process_group: pid })
}

pub(super) fn terminate(child: &mut Child, containment: &ChildContainment, grace: Duration) {
    let _ = signal_group(containment.process_group, libc::SIGTERM);
    let _ = wait_for_exit(child, grace);
    if child.try_wait().ok().flatten().is_none() || group_alive(containment.process_group) {
        let _ = signal_group(containment.process_group, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = wait_for_exit(child, grace);
}

pub(super) fn terminate_residual(containment: &ChildContainment, grace: Duration) {
    if !group_alive(containment.process_group) {
        return;
    }
    let _ = signal_group(containment.process_group, libc::SIGTERM);
    let deadline = Instant::now() + grace;
    while group_alive(containment.process_group) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    if group_alive(containment.process_group) {
        let _ = signal_group(containment.process_group, libc::SIGKILL);
    }
}

pub(super) fn terminate_detached(child: &mut Child, grace: Duration) -> std::io::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    let process_group = child.id();
    signal_group(process_group, libc::SIGTERM)?;
    let _ = wait_for_exit(child, grace)?;
    if child.try_wait()?.is_none() || group_alive(process_group) {
        signal_group(process_group, libc::SIGKILL)?;
    }
    let _ = child.kill();
    if wait_for_exit(child, grace)?.is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "process group did not exit after forced termination",
        ));
    }
    Ok(())
}

pub(super) fn terminate_process_group(pid: u32) -> std::io::Result<()> {
    signal_group(pid, libc::SIGTERM)
}

fn wait_for_exit(child: &mut Child, grace: Duration) -> std::io::Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + grace;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(20).min(deadline.duration_since(now)));
    }
}

fn signal_group(process_group: u32, signal: i32) -> std::io::Result<()> {
    let Ok(process_group) = i32::try_from(process_group) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "process group id does not fit platform pid type",
        ));
    };
    // SAFETY: kill is called with a validated process-group id and no Rust memory pointers.
    if unsafe { libc::kill(-process_group, signal) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(error)
}

fn group_alive(process_group: u32) -> bool {
    let Ok(process_group) = i32::try_from(process_group) else {
        return false;
    };
    // SAFETY: signal zero only probes a validated process-group id.
    let result = unsafe { libc::kill(-process_group, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::time::Duration;

    const FIXTURE_ENV: &str = "NEOMAX_PROCESS_GROUP_FIXTURE";
    const FIXTURE_EXITED: &str = "exited";
    const FIXTURE_PARENT: &str = "parent";
    const FIXTURE_DESCENDANT: &str = "descendant";

    #[test]
    fn termination_is_idempotent_after_the_child_has_exited() {
        if std::env::var(FIXTURE_ENV).ok().as_deref() == Some(FIXTURE_EXITED) {
            return;
        }
        let mut command = fixture_command(
            "io::process_group::unix::tests::termination_is_idempotent_after_the_child_has_exited",
            FIXTURE_EXITED,
        );
        command.current_dir(Path::new("/"));
        configure(&mut command);
        let mut child = command.spawn().unwrap();
        let containment = attach(child.id()).unwrap();
        child.wait().unwrap();
        terminate(&mut child, &containment, Duration::from_millis(1));
        terminate_residual(&containment, Duration::from_millis(1));
    }

    #[test]
    fn termination_reaps_a_descendant_in_the_process_group() {
        let temp = tempfile::tempdir().unwrap();
        if std::env::var(FIXTURE_ENV).ok().as_deref() == Some(FIXTURE_DESCENDANT) {
            thread::sleep(Duration::from_secs(1));
            let marker = PathBuf::from(
                std::env::var_os("NEOMAX_PROCESS_GROUP_FINISHED").expect("descendant marker path"),
            );
            fs::write(marker, b"finished").expect("write descendant marker");
            thread::sleep(Duration::from_secs(30));
            return;
        }
        if std::env::var(FIXTURE_ENV).ok().as_deref() == Some(FIXTURE_PARENT) {
            let started = PathBuf::from(
                std::env::var_os("NEOMAX_PROCESS_GROUP_STARTED")
                    .expect("parent started marker path"),
            );
            let marker = PathBuf::from(
                std::env::var_os("NEOMAX_PROCESS_GROUP_FINISHED").expect("descendant marker path"),
            );
            let executable = std::env::current_exe().expect("test executable");
            let mut descendant = Command::new(executable)
                .args([
                    "--exact",
                    "io::process_group::unix::tests::termination_reaps_a_descendant_in_the_process_group",
                    "--nocapture",
                ])
                .env(FIXTURE_ENV, FIXTURE_DESCENDANT)
                .env("NEOMAX_PROCESS_GROUP_FINISHED", marker)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn descendant fixture");
            fs::write(started, b"started").expect("write parent started marker");
            thread::sleep(Duration::from_secs(30));
            let _ = descendant.kill();
            let _ = descendant.wait();
            return;
        }
        let marker = temp.path().join("descendant-finished");
        let started = temp.path().join("descendant-started");
        let mut command = fixture_command(
            "io::process_group::unix::tests::termination_reaps_a_descendant_in_the_process_group",
            FIXTURE_PARENT,
        );
        command
            .env("NEOMAX_PROCESS_GROUP_STARTED", &started)
            .env("NEOMAX_PROCESS_GROUP_FINISHED", &marker)
            .current_dir(temp.path());
        configure(&mut command);
        let mut child = command.spawn().unwrap();
        let containment = attach(child.id()).unwrap();
        wait_for_marker(&started);
        terminate(&mut child, &containment, Duration::from_millis(30));
        terminate_residual(&containment, Duration::from_millis(30));
        thread::sleep(Duration::from_millis(1_100));
        assert!(!marker.exists());
    }

    fn fixture_command(test_name: &str, mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args(["--exact", test_name, "--nocapture"])
            .env(FIXTURE_ENV, mode)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    fn wait_for_marker(path: &Path) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !path.exists() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(path.exists(), "fixture did not reach its spawn handshake");
    }
}
