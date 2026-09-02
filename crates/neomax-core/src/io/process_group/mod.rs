use std::process::{Child, Command};
use std::time::Duration;

use crate::Result;

pub const DEFAULT_DETACHED_TERMINATE_GRACE: Duration = Duration::from_millis(500);

#[cfg(not(any(unix, windows)))]
mod other;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(not(any(unix, windows)))]
use self::other as platform;
#[cfg(unix)]
use self::unix as platform;
#[cfg(windows)]
use self::windows as platform;

pub use platform::ChildContainment;

pub trait ProcessControl: Send + Sync {
    fn terminate(&self, child: &mut Child, containment: &ChildContainment, grace: Duration);
    fn terminate_residual(&self, containment: &ChildContainment, grace: Duration);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcessControl;

impl ProcessControl for SystemProcessControl {
    fn terminate(&self, child: &mut Child, containment: &ChildContainment, grace: Duration) {
        platform::terminate(child, containment, grace);
    }

    fn terminate_residual(&self, containment: &ChildContainment, grace: Duration) {
        platform::terminate_residual(containment, grace);
    }
}

pub fn spawn_managed(command: &mut Command) -> Result<(Child, ChildContainment)> {
    platform::spawn_managed(command)
}

pub trait DetachedProcessControl: Send + Sync {
    fn terminate(&self, child: &mut Child, grace: Duration) -> std::io::Result<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemDetachedProcessControl;

impl DetachedProcessControl for SystemDetachedProcessControl {
    fn terminate(&self, child: &mut Child, grace: Duration) -> std::io::Result<()> {
        platform::terminate_detached(child, grace)
    }
}

pub fn spawn_detached(command: &mut Command) -> Result<Child> {
    platform::configure_detached(command);
    Ok(command.spawn()?)
}

pub fn terminate_detached(child: &mut Child) -> Result<()> {
    terminate_detached_with(
        child,
        DEFAULT_DETACHED_TERMINATE_GRACE,
        &SystemDetachedProcessControl,
    )
}

pub fn terminate_detached_with(
    child: &mut Child,
    grace: Duration,
    control: &dyn DetachedProcessControl,
) -> Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    control.terminate(child, grace)?;
    if child.try_wait()?.is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "detached process did not exit after forced termination",
        )
        .into());
    }
    Ok(())
}

pub fn terminate_worker(pid: u32) -> Result<()> {
    if pid <= 1 || pid == std::process::id() {
        return Err(crate::Error::InvalidArgument(format!(
            "refusing to terminate unsafe worker process id {pid}"
        )));
    }
    platform::terminate_process_group(pid)?;
    Ok(())
}

pub fn terminate_supervisor(pid: u32) -> Result<()> {
    if pid <= 1 || pid == std::process::id() {
        return Err(crate::Error::InvalidArgument(format!(
            "refusing to terminate unsafe supervisor process id {pid}"
        )));
    }
    platform::terminate_process_group(pid)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{
        DetachedProcessControl, spawn_detached, terminate_detached_with, terminate_supervisor,
        terminate_worker,
    };

    #[derive(Default)]
    struct FakeDetachedProcessControl {
        calls: AtomicUsize,
    }

    #[derive(Default)]
    struct NoopDetachedProcessControl;

    impl DetachedProcessControl for FakeDetachedProcessControl {
        fn terminate(&self, child: &mut Child, _grace: std::time::Duration) -> std::io::Result<()> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            child.kill()?;
            let _ = child.wait()?;
            Ok(())
        }
    }

    impl DetachedProcessControl for NoopDetachedProcessControl {
        fn terminate(
            &self,
            _child: &mut Child,
            _grace: std::time::Duration,
        ) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn detached_termination_uses_injected_control_and_reaps_child() {
        if std::env::var_os("NEOMAX_PROCESS_GROUP_FIXTURE").is_some() {
            std::thread::sleep(std::time::Duration::from_secs(30));
            return;
        }
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "io::process_group::tests::detached_termination_uses_injected_control_and_reaps_child",
                "--nocapture",
            ])
            .env("NEOMAX_PROCESS_GROUP_FIXTURE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = spawn_detached(&mut command).expect("spawn fixture child");
        let control = FakeDetachedProcessControl::default();
        terminate_detached_with(&mut child, std::time::Duration::from_millis(10), &control)
            .expect("terminate fixture child");
        assert_eq!(control.calls.load(Ordering::Relaxed), 1);
        assert!(child.try_wait().expect("inspect fixture child").is_some());
    }

    #[test]
    fn detached_termination_never_waits_unboundedly_after_control_returns() {
        if std::env::var_os("NEOMAX_PROCESS_GROUP_NOOP_FIXTURE").is_some() {
            std::thread::sleep(std::time::Duration::from_secs(30));
            return;
        }
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "io::process_group::tests::detached_termination_never_waits_unboundedly_after_control_returns",
                "--nocapture",
            ])
            .env("NEOMAX_PROCESS_GROUP_NOOP_FIXTURE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = spawn_detached(&mut command).expect("spawn fixture child");
        let started = std::time::Instant::now();
        let error = terminate_detached_with(
            &mut child,
            std::time::Duration::from_millis(10),
            &NoopDetachedProcessControl,
        )
        .expect_err("no-op control must not report a live process as terminated");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(error.to_string().contains("did not exit"));
        child.kill().expect("stop fixture child");
        child.wait().expect("reap fixture child");
    }

    #[test]
    fn pid_termination_rejects_processes_that_could_escape_ownership() {
        assert!(terminate_worker(0).is_err());
        assert!(terminate_worker(1).is_err());
        assert!(terminate_supervisor(0).is_err());
        assert!(terminate_supervisor(1).is_err());
        assert!(terminate_worker(std::process::id()).is_err());
        assert!(terminate_supervisor(std::process::id()).is_err());
    }
}
