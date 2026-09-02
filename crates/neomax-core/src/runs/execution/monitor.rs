use std::process::{Child, ExitStatus};
use std::thread;
use std::time::Instant;

use crate::Result;

use super::logs::AttemptLogFiles;
use super::types::{KilledFor, SupervisorConfig, SupervisorDirective};
use crate::io::process_group::{ChildContainment, ProcessControl};

pub(super) struct MonitorOutcome {
    pub exit_status: ExitStatus,
    pub killed_for: Option<KilledFor>,
    pub directive: Option<SupervisorDirective>,
}

pub(super) fn wait<M>(
    child: &mut Child,
    containment: &ChildContainment,
    logs: &AttemptLogFiles,
    config: &SupervisorConfig,
    process_control: &dyn ProcessControl,
    mut monitor: M,
) -> Result<MonitorOutcome>
where
    M: FnMut() -> Result<SupervisorDirective>,
{
    let started = Instant::now();
    let mut last_activity = started;
    let mut last_size = logs.size();
    loop {
        let observed_exit = child.try_wait()?;
        if observed_exit.is_none() {
            thread::sleep(config.poll_interval);
            let size = logs.size();
            if size != last_size {
                last_size = size;
                last_activity = Instant::now();
            }
        }
        let directive = match monitor() {
            Ok(value) => value,
            Err(error) => {
                if observed_exit.is_none() && child.try_wait()?.is_none() {
                    terminate(child, containment, config, process_control);
                }
                return Err(error);
            }
        };
        match directive {
            SupervisorDirective::Rotate(_) => {
                return terminate_with(
                    child,
                    containment,
                    config,
                    process_control,
                    KilledFor::Quota,
                    Some(directive),
                );
            }
            SupervisorDirective::Abort => {
                return terminate_with(
                    child,
                    containment,
                    config,
                    process_control,
                    KilledFor::Aborted,
                    Some(directive),
                );
            }
            SupervisorDirective::Continue => {
                if let Some(exit_status) = observed_exit.or(child.try_wait()?) {
                    return Ok(MonitorOutcome {
                        exit_status,
                        killed_for: None,
                        directive: None,
                    });
                }
            }
        }
        let now = Instant::now();
        if config
            .wall_timeout
            .is_some_and(|limit| now.duration_since(started) > limit)
        {
            return terminate_with(
                child,
                containment,
                config,
                process_control,
                KilledFor::Timeout,
                None,
            );
        }
        if config
            .stall_timeout
            .is_some_and(|limit| now.duration_since(last_activity) > limit)
        {
            return terminate_with(
                child,
                containment,
                config,
                process_control,
                KilledFor::Stalled,
                None,
            );
        }
    }
}

fn terminate_with(
    child: &mut Child,
    containment: &ChildContainment,
    config: &SupervisorConfig,
    process_control: &dyn ProcessControl,
    killed_for: KilledFor,
    directive: Option<SupervisorDirective>,
) -> Result<MonitorOutcome> {
    terminate(child, containment, config, process_control);
    Ok(MonitorOutcome {
        exit_status: child.wait()?,
        killed_for: Some(killed_for),
        directive,
    })
}

fn terminate(
    child: &mut Child,
    containment: &ChildContainment,
    config: &SupervisorConfig,
    process_control: &dyn ProcessControl,
) {
    process_control.terminate(child, containment, config.terminate_grace);
    process_control.terminate_residual(containment, config.terminate_grace);
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::io::process_group::{self, ProcessControl};

    const FIXTURE_ENV: &str = "NEOMAX_MONITOR_FIXTURE";
    const FIXTURE_SLEEP: &str = "sleep";
    const FIXTURE_EXIT: &str = "exit";

    #[derive(Default)]
    struct FakeProcessControl {
        terminate_calls: AtomicUsize,
        residual_calls: AtomicUsize,
    }

    impl ProcessControl for FakeProcessControl {
        fn terminate(&self, child: &mut Child, _containment: &ChildContainment, _grace: Duration) {
            self.terminate_calls.fetch_add(1, Ordering::Relaxed);
            let _ = child.kill();
        }

        fn terminate_residual(&self, _containment: &ChildContainment, _grace: Duration) {
            self.residual_calls.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn control_directive_uses_injected_process_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let logs = AttemptLogFiles::open(temp.path(), "run", 1).unwrap();
        if std::env::var(FIXTURE_ENV).ok().as_deref() == Some(FIXTURE_SLEEP) {
            thread::sleep(Duration::from_secs(30));
            return;
        }
        let mut command = fixture_command(
            "runs::execution::monitor::tests::control_directive_uses_injected_process_cleanup",
            FIXTURE_SLEEP,
        );
        command.current_dir(temp.path());
        let (mut child, containment) = process_group::spawn_managed(&mut command).unwrap();
        let control = FakeProcessControl::default();
        let config = SupervisorConfig {
            poll_interval: Duration::from_millis(1),
            terminate_grace: Duration::from_millis(1),
            wall_timeout: None,
            stall_timeout: None,
        };
        let outcome = wait(&mut child, &containment, &logs, &config, &control, || {
            Ok(SupervisorDirective::Abort)
        })
        .unwrap();
        assert_eq!(outcome.killed_for, Some(KilledFor::Aborted));
        assert_eq!(control.terminate_calls.load(Ordering::Relaxed), 1);
        assert_eq!(control.residual_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn abort_directive_wins_when_the_child_exits_before_the_callback() {
        if std::env::var(FIXTURE_ENV).ok().as_deref() == Some(FIXTURE_EXIT) {
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let logs = AttemptLogFiles::open(temp.path(), "run", 1).unwrap();
        let mut command = fixture_command(
            "runs::execution::monitor::tests::abort_directive_wins_when_the_child_exits_before_the_callback",
            FIXTURE_EXIT,
        );
        command.current_dir(temp.path());
        let (mut child, containment) = process_group::spawn_managed(&mut command).unwrap();
        assert!(child.wait().unwrap().success());
        let control = FakeProcessControl::default();
        let config = SupervisorConfig {
            poll_interval: Duration::from_millis(1),
            terminate_grace: Duration::from_millis(1),
            wall_timeout: None,
            stall_timeout: None,
        };
        let outcome = wait(&mut child, &containment, &logs, &config, &control, || {
            Ok(SupervisorDirective::Abort)
        })
        .unwrap();
        assert_eq!(outcome.killed_for, Some(KilledFor::Aborted));
        assert_eq!(control.terminate_calls.load(Ordering::Relaxed), 1);
        assert_eq!(control.residual_calls.load(Ordering::Relaxed), 1);
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
}
