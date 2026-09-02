use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::io::{MAX_COMMAND_OUTPUT_BYTES, read_capped};

pub(crate) const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

type CapturedOutput = std::io::Result<(Vec<u8>, bool)>;
type OutputReader = thread::JoinHandle<CapturedOutput>;

pub(crate) trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> Result<Output>;
}

#[derive(Debug, Default)]
pub(crate) struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> Result<Output> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("start {program}"))?;
        let mut stdout = child.stdout.take().map(|mut pipe| {
            thread::spawn(move || read_capped(&mut pipe, MAX_COMMAND_OUTPUT_BYTES))
        });
        let mut stderr = child.stderr.take().map(|mut pipe| {
            thread::spawn(move || read_capped(&mut pipe, MAX_COMMAND_OUTPUT_BYTES))
        });
        let started = Instant::now();
        loop {
            if let Some(status) = child
                .try_wait()
                .with_context(|| format!("wait for {program}"))?
            {
                let _ = child.wait();
                let stdout = join_output(stdout.take())
                    .with_context(|| format!("collect {program} output"))?;
                let stderr = join_output(stderr.take())
                    .with_context(|| format!("collect {program} error"))?;
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_output(stdout.take());
                let _ = join_output(stderr.take());
                bail!("{program} timed out after {} seconds", timeout.as_secs());
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

fn join_output(handle: Option<OutputReader>) -> Result<Vec<u8>> {
    let Some(handle) = handle else {
        return Ok(Vec::new());
    };
    let (bytes, exceeded) = handle
        .join()
        .map_err(|_| anyhow::anyhow!("command output reader panicked"))??;
    if exceeded {
        bail!("command output exceeded the local read limit");
    }
    Ok(bytes)
}

pub(crate) fn success(output: &Output) -> bool {
    output.status.success()
}

#[cfg(all(test, unix))]
mod tests {
    use super::{CommandRunner, SystemRunner};
    use std::time::Duration;

    #[test]
    fn system_runner_returns_when_a_command_exceeds_its_deadline() {
        let error = SystemRunner
            .run("sh", &["-c", "sleep 1"], Duration::from_millis(10))
            .expect_err("sleep must exceed the deadline");
        assert!(error.to_string().contains("timed out"));
    }
}
