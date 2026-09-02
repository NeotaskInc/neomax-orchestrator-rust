use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use neomax_core::atomic::write_json_atomic;
use serde::{Deserialize, Serialize};

use crate::context::RuntimeContext;
use crate::process::terminate_detached;

const HANDSHAKE_DIR: &str = "launch-handshakes";
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const HANDSHAKE_POLL: Duration = Duration::from_millis(25);
const MAX_HANDSHAKE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LaunchHandshake {
    pub(crate) status: String,
    pub(crate) run_id: Option<String>,
    pub(crate) error: Option<String>,
}

impl LaunchHandshake {
    pub(crate) fn started(run_id: impl Into<String>) -> Self {
        Self {
            status: "started".into(),
            run_id: Some(run_id.into()),
            error: None,
        }
    }

    pub(crate) fn failed(error: impl Into<String>) -> Self {
        Self {
            status: "error".into(),
            run_id: None,
            error: Some(error.into()),
        }
    }
}

pub(crate) fn create_path(context: &RuntimeContext) -> Result<PathBuf> {
    let directory = context.paths.state.join(HANDSHAKE_DIR);
    fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(directory.join(format!("{}-{nanos}.json", std::process::id())))
}

pub(crate) fn path_from_environment() -> Option<PathBuf> {
    std::env::var_os("NEOMAX_LAUNCH_HANDSHAKE").map(PathBuf::from)
}

pub(crate) fn write(path: &Path, handshake: &LaunchHandshake) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_json_atomic(path, handshake).map_err(Into::into)
}

pub(crate) fn write_error(error: impl Into<String>) {
    let Some(path) = path_from_environment() else {
        return;
    };
    let _ = write(&path, &LaunchHandshake::failed(error));
}

pub(crate) fn wait(path: &Path, child: &mut Child) -> Result<LaunchHandshake> {
    wait_with_timeout(path, child, HANDSHAKE_TIMEOUT)
}

fn wait_with_timeout(path: &Path, child: &mut Child, timeout: Duration) -> Result<LaunchHandshake> {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if path.is_file() {
            let metadata = match fs::metadata(path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    return Err(abort(
                        path,
                        child,
                        anyhow::Error::new(error).context(format!(
                            "could not inspect launch handshake {}",
                            path.display()
                        )),
                    ));
                }
            };
            if metadata.len() > MAX_HANDSHAKE_BYTES {
                return Err(abort(
                    path,
                    child,
                    anyhow::anyhow!("detached launch handshake is too large"),
                ));
            }
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Err(abort(
                        path,
                        child,
                        anyhow::Error::new(error).context(format!(
                            "could not read launch handshake {}",
                            path.display()
                        )),
                    ));
                }
            };
            let handshake = match serde_json::from_slice(&bytes)
                .with_context(|| format!("invalid launch handshake {}", path.display()))
            {
                Ok(handshake) => handshake,
                Err(error) => return Err(abort(path, child, error)),
            };
            cleanup(path);
            return Ok(handshake);
        }
        if let Some(status) = child
            .try_wait()
            .with_context(|| "could not inspect detached supervisor status")?
        {
            cleanup(path);
            anyhow::bail!("detached launch exited before startup handshake ({status})");
        }
        std::thread::sleep(HANDSHAKE_POLL);
    }
    cleanup(path);
    let timeout_error = anyhow::anyhow!(
        "detached launch did not acknowledge startup within {} seconds",
        timeout.as_secs()
    );
    match terminate_detached(child) {
        Ok(()) => Err(timeout_error),
        Err(error) => Err(anyhow::anyhow!(
            "{timeout_error}; could not terminate detached supervisor: {error}"
        )),
    }
}

pub(crate) fn cleanup(path: &Path) {
    let _ = fs::remove_file(path);
}

fn abort(path: &Path, child: &mut Child, error: anyhow::Error) -> anyhow::Error {
    cleanup(path);
    match terminate_detached(child) {
        Ok(()) => error,
        Err(stop_error) => {
            anyhow::anyhow!("{error}; could not terminate detached supervisor: {stop_error}")
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::io::Write;
    #[cfg(unix)]
    use std::process::{Command, Stdio};
    #[cfg(unix)]
    use std::thread;

    use super::*;

    #[test]
    fn handshake_round_trips_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("launch.json");
        write(&path, &LaunchHandshake::started("run-1")).unwrap();
        let handshake =
            serde_json::from_slice::<LaunchHandshake>(&fs::read(path).unwrap()).unwrap();
        assert_eq!(handshake.status, "started");
        assert_eq!(handshake.run_id.as_deref(), Some("run-1"));
    }

    #[cfg(unix)]
    #[test]
    fn wait_polls_until_a_child_publishes_a_durable_run_id() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("launch.json");
        let fixture_script = r#"
IFS= read -r trigger || exit 10
[ "$trigger" = publish ] || exit 11
temporary="$NEOMAX_HANDSHAKE_FIXTURE_PATH.tmp.$$"
printf '%s\n' '{"status":"started","run_id":"run-1","error":null}' > "$temporary"
mv "$temporary" "$NEOMAX_HANDSHAKE_FIXTURE_PATH"
IFS= read -r finish || true
"#;
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", fixture_script])
            .env("NEOMAX_HANDSHAKE_FIXTURE_PATH", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = crate::process::spawn_detached(&mut command).unwrap();
        let mut child_stdin = child.stdin.take().unwrap();
        writeln!(child_stdin, "publish").unwrap();
        child_stdin.flush().unwrap();
        let handshake = wait(&path, &mut child).unwrap();
        assert_eq!(handshake.run_id.as_deref(), Some("run-1"));
        drop(child_stdin);
        reap_fixture(&mut child);
    }

    #[cfg(unix)]
    fn reap_fixture(child: &mut Child) {
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "fixture exited unsuccessfully: {status}");
                return;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let status = child.wait().unwrap();
                panic!("fixture did not exit after stdin closed: {status}");
            }
            thread::yield_now();
        }
    }

    #[cfg(unix)]
    #[test]
    fn wait_reports_exit_and_bounds_a_nonresponsive_child() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("launch.json");
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "sleep 5"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = crate::process::spawn_detached(&mut command).unwrap();
        let error = wait_with_timeout(&path, &mut child, Duration::from_millis(30))
            .expect_err("startup must be bounded");
        assert!(error.to_string().contains("did not acknowledge startup"));
        assert!(child.try_wait().unwrap().is_some());
    }
}
