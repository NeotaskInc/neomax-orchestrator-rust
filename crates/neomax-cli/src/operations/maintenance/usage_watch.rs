use std::env;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};

use anyhow::{Context, Result, bail};
use neomax_core::providers::scrub_provider_environment;
use neomax_core::runtime;

use crate::context::RuntimeContext;
use crate::error;
use crate::parser;

pub(super) trait UsageAgentRunner {
    fn run(&self, program: &Path, args: &[String], context: &RuntimeContext) -> Result<ExitStatus>;
}

struct SystemUsageAgentRunner;

impl UsageAgentRunner for SystemUsageAgentRunner {
    fn run(&self, program: &Path, args: &[String], context: &RuntimeContext) -> Result<ExitStatus> {
        let mut command = runtime::process_command(program, args, &context.cwd)?;
        scrub_provider_environment(&mut command);
        command
            .env("NEOMAX_HOME", &context.paths.state)
            .env("NEOMAX_CLI_BIN", current_cli_binary()?);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        command
            .status()
            .with_context(|| format!("start usage agent {}", program.display()))
    }
}

pub(super) fn run(args: &[String], context: &RuntimeContext) -> Result<()> {
    run_with_runner(args, context, &SystemUsageAgentRunner)
}

pub(super) fn run_with_runner(
    args: &[String],
    context: &RuntimeContext,
    runner: &dyn UsageAgentRunner,
) -> Result<()> {
    let once = parser::has(args, "--once");
    let rebuild = parser::has(args, "--rebuild");
    let no_backfill = parser::has(args, "--no-backfill");
    let json = parser::has(args, "--json");
    error::usage(validate_args(args))?;
    let program = usage_agent_binary()?;
    let mut child_args = vec![if once { "once" } else { "run" }.to_owned()];
    if rebuild {
        child_args.push("--rebuild".into());
    }
    if no_backfill {
        child_args.push("--no-backfill".into());
    }
    if json {
        child_args.push("--json".into());
    }
    let status = runner.run(&program, &child_args, context)?;
    if status.success() {
        return Ok(());
    }
    bail!(
        "usage-watch: usage agent exited with {}",
        status
            .code()
            .map_or_else(|| "a signal".into(), |code| code.to_string())
    )
}

fn validate_args(args: &[String]) -> Result<()> {
    for arg in args {
        if !matches!(
            arg.as_str(),
            "--once" | "--rebuild" | "--no-backfill" | "--json"
        ) {
            bail!("usage-watch: unknown option {arg}");
        }
    }
    Ok(())
}

fn usage_agent_binary() -> Result<PathBuf> {
    if let Some(value) = env::var_os("NEOMAX_USAGE_AGENT_BIN") {
        let path = PathBuf::from(value);
        if path.as_os_str().is_empty() {
            bail!("NEOMAX_USAGE_AGENT_BIN must not be empty");
        }
        return Ok(path);
    }
    if let Ok(current) = env::current_exe() {
        if let Some(parent) = current.parent() {
            let sibling = parent.join(if cfg!(windows) {
                "neomax-usage-agent.exe"
            } else {
                "neomax-usage-agent"
            });
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
    }
    Ok(PathBuf::from(if cfg!(windows) {
        "neomax-usage-agent.exe"
    } else {
        "neomax-usage-agent"
    }))
}

fn current_cli_binary() -> Result<PathBuf> {
    env::var_os("NEOMAX_CLI_BIN")
        .map(PathBuf::from)
        .or_else(|| env::current_exe().ok())
        .context("could not determine the Neomax CLI executable")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::ExitStatus;
    use std::sync::Mutex;

    use super::*;
    use crate::tests::fixture;

    struct FakeRunner {
        calls: Mutex<Vec<(PathBuf, Vec<String>)>>,
        status: ExitStatus,
    }

    impl UsageAgentRunner for FakeRunner {
        fn run(
            &self,
            program: &Path,
            args: &[String],
            _context: &RuntimeContext,
        ) -> Result<ExitStatus> {
            self.calls
                .lock()
                .expect("calls")
                .push((program.to_path_buf(), args.to_vec()));
            Ok(self.status)
        }
    }

    #[cfg(unix)]
    fn success() -> ExitStatus {
        std::os::unix::process::ExitStatusExt::from_raw(0)
    }

    #[cfg(windows)]
    fn success() -> ExitStatus {
        std::os::windows::process::ExitStatusExt::from_raw(0)
    }

    #[cfg(unix)]
    fn failure() -> ExitStatus {
        std::os::unix::process::ExitStatusExt::from_raw(7 << 8)
    }

    #[cfg(windows)]
    fn failure() -> ExitStatus {
        std::os::windows::process::ExitStatusExt::from_raw(7)
    }

    #[test]
    fn once_maps_to_the_usage_agent_once_subcommand() {
        let fixture = fixture();
        let runner = FakeRunner {
            calls: Mutex::new(Vec::new()),
            status: success(),
        };
        run_with_runner(
            &["--once".into(), "--rebuild".into(), "--json".into()],
            &fixture.context,
            &runner,
        )
        .unwrap();
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls[0].1, ["once", "--rebuild", "--json"]);
    }

    #[test]
    fn regular_watch_maps_to_run_and_reports_failure() {
        let fixture = fixture();
        let runner = FakeRunner {
            calls: Mutex::new(Vec::new()),
            status: failure(),
        };
        let error = run_with_runner(&[], &fixture.context, &runner).unwrap_err();
        assert!(error.to_string().contains("usage agent exited"));
        assert_eq!(runner.calls.lock().unwrap()[0].1, ["run"]);
    }

    #[test]
    fn unknown_flags_are_rejected_before_process_launch() {
        let fixture = fixture();
        let runner = FakeRunner {
            calls: Mutex::new(Vec::new()),
            status: success(),
        };
        assert!(run_with_runner(&["--network".into()], &fixture.context, &runner).is_err());
        assert!(runner.calls.lock().unwrap().is_empty());
    }
}
