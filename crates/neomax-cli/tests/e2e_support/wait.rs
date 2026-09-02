use std::fs;
use std::process::ExitStatus;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use super::harness::E2eHarness;
use super::process::E2eChild;

#[allow(dead_code)]
pub fn process_test_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[allow(dead_code)]
pub fn wait_for_run<F>(harness: &E2eHarness, predicate: F) -> (String, serde_json::Value)
where
    F: Fn(&serde_json::Value) -> bool,
{
    let mut matching_run = None;
    let attempts = if cfg!(windows) { 1_200 } else { 400 };
    'poll: for _ in 0..attempts {
        if let Ok(entries) = fs::read_dir(harness.state.join("runs")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(contents) = fs::read_to_string(&path) {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) {
                        if predicate(&value) {
                            let id = value["id"].as_str().expect("run id").to_owned();
                            matching_run = Some((id, value));
                            break 'poll;
                        }
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    matching_run.unwrap_or_else(|| {
        panic!(
            "run did not reach the requested state; run snapshot:\n{}",
            run_snapshot(harness)
        )
    })
}

#[allow(dead_code)]
pub fn wait_for_run_or_child_exit<F>(
    harness: &E2eHarness,
    child: &mut E2eChild,
    predicate: F,
) -> (String, serde_json::Value)
where
    F: Fn(&serde_json::Value) -> bool,
{
    let attempts = if cfg!(windows) { 1_200 } else { 400 };
    for _ in 0..attempts {
        if let Some(run) = matching_run(harness, &predicate) {
            return run;
        }
        if let Some(status) = child.try_wait().expect("inspect neomax child") {
            panic!(
                "neomax exited before the run reached the requested state ({status})\n{}",
                child.diagnostics()
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!(
        "run did not reach the requested state; child diagnostics:\n{}",
        child.diagnostics()
    )
}

fn matching_run<F>(harness: &E2eHarness, predicate: &F) -> Option<(String, serde_json::Value)>
where
    F: Fn(&serde_json::Value) -> bool,
{
    let entries = fs::read_dir(harness.state.join("runs")).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) else {
            continue;
        };
        if predicate(&value) {
            let Some(id) = value["id"].as_str().map(str::to_owned) else {
                continue;
            };
            return Some((id, value));
        }
    }
    None
}

fn run_snapshot(harness: &E2eHarness) -> String {
    let mut snapshots = fs::read_dir(harness.state.join("runs"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .collect::<Vec<_>>();
    snapshots.sort();
    snapshots.join("\n")
}

#[allow(dead_code)]
pub fn wait_for_exit(child: &mut E2eChild) -> ExitStatus {
    for _ in 0..200 {
        if let Some(status) = child.try_wait().expect("poll neomax") {
            return status;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    neomax_core::io::process_group::terminate_detached(child.as_child_mut())
        .expect("stop timed out neomax");
    child.wait().expect("wait neomax")
}

#[allow(dead_code)]
pub fn terminate_pid(pid: u32) {
    if pid <= 1 || pid == std::process::id() {
        panic!("refusing to terminate unsafe fixture process id {pid}");
    }
    #[cfg(unix)]
    terminate_pid_unix(pid);
    #[cfg(windows)]
    terminate_pid_windows(pid);
}

#[allow(dead_code)]
pub fn wait_for_pid_exit(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while process_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(!process_alive(pid), "fixture process {pid} did not exit");
}

#[cfg(unix)]
#[allow(dead_code)]
fn process_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal zero only probes the validated fixture PID.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
#[allow(dead_code)]
fn process_alive(pid: u32) -> bool {
    use std::process::{Command, Stdio};

    let output = Command::new(windows_system_tool("tasklist.exe"))
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    output.is_ok_and(|output| {
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| tasklist_line_pid(line) == Some(pid))
    })
}

#[cfg(windows)]
fn tasklist_line_pid(line: &str) -> Option<u32> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    if let Some(csv) = line.strip_prefix('"') {
        let (_, fields) = csv.split_once("\",\"")?;
        let (pid, _) = fields.split_once("\",\"")?;
        return pid.parse().ok();
    }
    line.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(all(test, windows))]
mod tests {
    use super::tasklist_line_pid;

    #[test]
    fn tasklist_pid_matching_uses_the_pid_field() {
        assert_eq!(
            tasklist_line_pid("\"worker,fixture.exe\",\"42\",\"Console\",\"1\",\"1,000 K\""),
            Some(42)
        );
        assert_eq!(
            tasklist_line_pid("worker.exe 42 Console 1 1,000 K"),
            Some(42)
        );
        assert_eq!(
            tasklist_line_pid("worker.exe 142 Console 1 1,000 K"),
            Some(142)
        );
        assert_ne!(
            tasklist_line_pid("worker.exe 142 Console 1 1,000 K"),
            Some(42)
        );
        assert_eq!(tasklist_line_pid("INFO: No tasks are running"), None);
    }
}

#[cfg(unix)]
#[allow(dead_code)]
fn terminate_pid_unix(pid: u32) {
    let Ok(platform_pid) = i32::try_from(pid) else {
        panic!("fixture process id does not fit platform pid type: {pid}");
    };
    if !process_alive(pid) {
        return;
    }
    // SAFETY: the PID came from a fixture handoff report and was validated above.
    let result = unsafe { libc::kill(platform_pid, libc::SIGTERM) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        assert_eq!(error.raw_os_error(), Some(libc::ESRCH), "terminate fixture");
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    if process_alive(pid) {
        // SAFETY: the PID remains the fixture child we just terminated.
        let result = unsafe { libc::kill(platform_pid, libc::SIGKILL) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            assert_eq!(error.raw_os_error(), Some(libc::ESRCH), "kill fixture");
        }
    }
}

#[cfg(windows)]
#[allow(dead_code)]
fn terminate_pid_windows(pid: u32) {
    use std::process::{Command, Stdio};

    if !process_alive(pid) {
        return;
    }
    let status = Command::new(windows_system_tool("taskkill.exe"))
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("terminate fixture process");
    assert!(
        status.success(),
        "taskkill failed for fixture process {pid}"
    );
}

#[cfg(windows)]
fn windows_system_tool(name: &str) -> std::path::PathBuf {
    let root = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("WINDIR"))
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute() && !neomax_core::io::is_rooted_but_not_absolute(path))
        .expect("absolute Windows system root");
    let tool = root.join("System32").join(name);
    assert!(
        tool.is_file(),
        "missing Windows system tool {}",
        tool.display()
    );
    tool
}
