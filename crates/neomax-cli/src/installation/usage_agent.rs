use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use neomax_core::installation::{InstallPaths, InstallReport};
use neomax_core::providers::scrub_provider_environment;
use neomax_core::runtime;

pub(super) const NO_USAGE_AGENT_ENV: &str = "NEOMAX_NO_USAGE_AGENT";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) trait CommandRunner: Send + Sync {
    fn run(
        &self,
        program: &Path,
        args: &[String],
        environment: &[(String, String)],
        timeout: Duration,
    ) -> Result<ExitStatus>;
}

#[derive(Debug, Default)]
pub(super) struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(
        &self,
        program: &Path,
        args: &[String],
        environment: &[(String, String)],
        timeout: Duration,
    ) -> Result<ExitStatus> {
        let current_dir = std::env::current_dir().unwrap_or_default();
        let mut command = runtime::process_command(program, args, &current_dir)?;
        scrub_provider_environment(&mut command);
        command
            .envs(environment.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .with_context(|| format!("start {}", program.display()))?;
        wait_with_timeout(&mut child, program, timeout)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ActionOutcome {
    Skipped,
    Completed,
    Failed(String),
}

pub(super) fn install_after_transaction(
    report: &InstallReport,
    opted_out: bool,
    runner: &dyn CommandRunner,
) -> ActionOutcome {
    if opted_out || !supported_platform() {
        return ActionOutcome::Skipped;
    }
    run_action(
        &report.bin_dir,
        "install",
        runner,
        "activate the automatic usage and rotation service",
    )
}

pub(super) fn uninstall_before_transaction(
    paths: &InstallPaths,
    runner: &dyn CommandRunner,
) -> ActionOutcome {
    if !supported_platform() {
        return ActionOutcome::Skipped;
    }
    if !paths.manifest_path().is_file() {
        return ActionOutcome::Skipped;
    }
    let usage_agent = usage_agent_path(&paths.bin_dir);
    if !usage_agent.is_file() {
        return ActionOutcome::Skipped;
    }
    run_action(
        &paths.bin_dir,
        "uninstall",
        runner,
        "stop the automatic usage and rotation service",
    )
}

pub(super) fn warning(action: &str, outcome: &ActionOutcome) -> Option<String> {
    let ActionOutcome::Failed(detail) = outcome else {
        return None;
    };
    let install = action.starts_with("activate");
    Some(format!(
        "[neomax] WARN could not {action}; {}. Run `neomax-usage-agent {}` manually when the service manager is available: {detail}",
        if install {
            "the file installation remains valid"
        } else {
            "owned files will still be removed; the service may need manual cleanup"
        },
        if install { "install" } else { "uninstall" }
    ))
}

pub(super) fn opted_out_from_environment() -> bool {
    std::env::var_os(NO_USAGE_AGENT_ENV).is_some()
}

fn run_action(
    bin_dir: &Path,
    action: &str,
    runner: &dyn CommandRunner,
    description: &str,
) -> ActionOutcome {
    let program = usage_agent_path(bin_dir);
    let cli = neomax_path(bin_dir);
    let args = vec![action.to_owned()];
    let environment = vec![
        ("NEOMAX_CLI_BIN".to_owned(), cli.display().to_string()),
        (
            "NEOMAX_USAGE_AGENT_BIN".to_owned(),
            program.display().to_string(),
        ),
    ];
    match runner.run(&program, &args, &environment, COMMAND_TIMEOUT) {
        Ok(status) if status.success() => ActionOutcome::Completed,
        Ok(status) => ActionOutcome::Failed(format!(
            "{}: {description} exited with {}",
            program.display(),
            status
                .code()
                .map_or_else(|| "a signal".to_owned(), |code| code.to_string())
        )),
        Err(error) => ActionOutcome::Failed(format!("{}: {error}", program.display())),
    }
}

fn usage_agent_path(bin_dir: &Path) -> PathBuf {
    bin_dir.join(binary_name("neomax-usage-agent"))
}

fn neomax_path(bin_dir: &Path) -> PathBuf {
    bin_dir.join(binary_name("neomax"))
}

fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn supported_platform() -> bool {
    cfg!(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "windows"
    ))
}

fn wait_with_timeout(child: &mut Child, program: &Path, timeout: Duration) -> Result<ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("wait for {}", program.display()))?
        {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "{} timed out after {} seconds",
                program.display(),
                timeout.as_secs()
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::ExitStatus;
    use std::sync::Mutex;
    use std::time::Duration;

    use neomax_core::installation::{InstallPaths, InstallReport};

    use super::{
        ActionOutcome, CommandRunner, NO_USAGE_AGENT_ENV, install_after_transaction, neomax_path,
        uninstall_before_transaction, usage_agent_path, warning,
    };

    #[cfg(unix)]
    use super::SystemRunner;

    type CommandCall = (PathBuf, Vec<String>, Vec<(String, String)>, Duration);

    struct FakeRunner {
        calls: Mutex<Vec<CommandCall>>,
        status: ExitStatus,
    }

    impl FakeRunner {
        fn successful() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                status: success_status(),
            }
        }

        fn failing() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                status: failure_status(),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            program: &Path,
            args: &[String],
            environment: &[(String, String)],
            timeout: Duration,
        ) -> anyhow::Result<ExitStatus> {
            self.calls.lock().expect("calls").push((
                program.to_path_buf(),
                args.to_vec(),
                environment.to_vec(),
                timeout,
            ));
            Ok(self.status)
        }
    }

    #[cfg(unix)]
    fn success_status() -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    }

    #[cfg(windows)]
    fn success_status() -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    }

    #[cfg(unix)]
    fn failure_status() -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(9 << 8)
    }

    #[cfg(windows)]
    fn failure_status() -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(9)
    }

    fn report(bin_dir: &Path) -> InstallReport {
        InstallReport {
            product: "neomax".into(),
            version: "test".into(),
            bin_dir: bin_dir.to_path_buf(),
            share_dir: bin_dir.join("../share/neomax"),
            aliases: Vec::new(),
            auxiliaries: Vec::new(),
            upgraded: false,
        }
    }

    #[test]
    fn install_uses_the_newly_installed_agent_and_exact_argv() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let runner = FakeRunner::successful();
        let outcome = install_after_transaction(&report(&bin), false, &runner);
        assert_eq!(outcome, ActionOutcome::Completed);
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, usage_agent_path(&bin));
        assert_eq!(calls[0].1, ["install"]);
        assert_eq!(calls[0].3, Duration::from_secs(15));
        assert_eq!(
            calls[0].2,
            [
                (
                    "NEOMAX_CLI_BIN".into(),
                    neomax_path(&bin).display().to_string()
                ),
                (
                    "NEOMAX_USAGE_AGENT_BIN".into(),
                    usage_agent_path(&bin).display().to_string(),
                ),
            ]
        );
    }

    #[test]
    fn opt_out_does_not_invoke_the_service_manager() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        let runner = FakeRunner::successful();
        let outcome = install_after_transaction(&report(&bin), true, &runner);
        assert_eq!(outcome, ActionOutcome::Skipped);
        assert!(runner.calls.lock().unwrap().is_empty());
        assert_eq!(NO_USAGE_AGENT_ENV, "NEOMAX_NO_USAGE_AGENT");
    }

    #[test]
    fn failed_activation_is_nonfatal_and_actionable() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        let runner = FakeRunner::failing();
        let outcome = install_after_transaction(&report(&bin), false, &runner);
        assert!(matches!(outcome, ActionOutcome::Failed(_)));
        let message = warning(
            "activate the automatic usage and rotation service",
            &outcome,
        )
        .expect("failed activation warning");
        assert!(message.contains("file installation remains valid"));
        assert!(message.contains("neomax-usage-agent install"));
    }

    #[test]
    fn uninstall_calls_the_agent_before_owned_files_are_removed() {
        let temp = tempfile::tempdir().unwrap();
        let paths = InstallPaths::new(
            temp.path().join("root"),
            temp.path().join("root/bin"),
            temp.path().join("root/share/neomax"),
        )
        .unwrap();
        std::fs::create_dir_all(&paths.bin_dir).unwrap();
        std::fs::create_dir_all(&paths.share_dir).unwrap();
        std::fs::write(paths.manifest_path(), b"owned").unwrap();
        std::fs::write(usage_agent_path(&paths.bin_dir), b"agent").unwrap();
        let runner = FakeRunner::successful();
        let outcome = uninstall_before_transaction(&paths, &runner);
        assert_eq!(outcome, ActionOutcome::Completed);
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, usage_agent_path(&paths.bin_dir));
        assert_eq!(calls[0].1, ["uninstall"]);
    }

    #[test]
    fn runner_contract_never_contains_provider_commands() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        let runner = FakeRunner::successful();
        let _ = install_after_transaction(&report(&bin), false, &runner);
        let calls = runner.calls.lock().unwrap();
        let expected_program = usage_agent_path(&bin);
        assert!(
            calls.iter().all(|(program, args, _, _)| {
                program == &expected_program && args == &["install"]
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn system_runner_is_bounded() {
        let error = SystemRunner
            .run(
                Path::new("sh"),
                &["-c".into(), "sleep 1".into()],
                &[],
                Duration::from_millis(10),
            )
            .expect_err("the bounded runner must stop a slow service command");
        assert!(error.to_string().contains("timed out"));
    }
}
