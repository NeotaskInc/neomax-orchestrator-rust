use std::time::Duration;

use super::super::{BoundedIoError, LocalProcessRunner, ProcessRequest, ProcessRunner};

fn child_request(mode: &str) -> ProcessRequest {
    ProcessRequest::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "io::tests::process::child_process_fixture",
            "--nocapture",
        ])
        .env("NEOMAX_BOUNDED_IO_CHILD", mode)
        .timeout(Duration::from_millis(150))
        .stdout_limit(64)
        .stderr_limit(64)
}

#[test]
fn process_output_is_bounded() {
    let output = LocalProcessRunner::new()
        .capture(&child_request("large"))
        .unwrap();
    assert!(output.stdout_truncated);
    assert!(!output.success);
    assert!(matches!(
        output.strict(&child_request("large")),
        Err(BoundedIoError::Truncated { .. })
    ));
}

#[test]
fn process_error_output_is_bounded() {
    let request = child_request("stderr");
    let output = LocalProcessRunner::new().capture(&request).unwrap();
    assert!(output.stderr_truncated);
    assert!(!output.success);
    assert!(matches!(
        output.strict(&request),
        Err(BoundedIoError::Truncated { .. })
    ));
}

#[test]
fn hanging_process_is_terminated_and_reaped() {
    let request = child_request("hang");
    let started = std::time::Instant::now();
    let output = LocalProcessRunner::new().capture(&request).unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(output.timed_out);
    assert!(matches!(
        output.strict(&request),
        Err(BoundedIoError::Timeout { .. })
    ));
}

#[test]
fn nonzero_exit_is_distinguished_from_io_limits() {
    let request = child_request("fail").timeout(Duration::from_secs(1));
    let output = LocalProcessRunner::new().capture(&request).unwrap();
    assert!(!output.success);
    assert!(matches!(
        output.strict(&request),
        Err(BoundedIoError::ProcessFailed { .. })
    ));
}

#[cfg(windows)]
#[test]
fn windows_batch_processes_round_trip_provider_arguments() {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("provider & fixture.cmd");
    let source = directory.path().join("argv_probe.rs");
    let probe = directory.path().join("argv-probe.exe");
    std::fs::write(
        &source,
        b"fn main() { let args = std::env::args().skip(1).collect::<Vec<_>>(); assert_eq!(args.len(), 1); print!(\"{}\", args[0]); }\n",
    )
    .unwrap();
    let compiled = std::process::Command::new("rustc")
        .args([source.as_os_str(), std::ffi::OsStr::new("-o"), probe.as_os_str()])
        .status()
        .unwrap();
    assert!(compiled.success());
    std::fs::write(&script, b"@echo off\r\n\"%~dp0argv-probe.exe\" %*\r\n").unwrap();

    let cases = [
        "",
        "plain",
        "two words",
        "before\tafter",
        r#"trailing\"#,
        r#"say "hello""#,
        "100% %% %PATH% %NEOMAX_MISSING%",
        "! ^ & | < > (parentheses)",
        r#"{"ultracode":"value & | < > (marker) ^ !"}"#,
        "unicode ☃ 한국어",
        "value & echo injected>neomax-injected-marker",
        "first\r\nsecond\nthird\rfourth",
    ];
    for value in cases {
        let request = ProcessRequest::new(&script)
            .arg(value)
            .cwd(directory.path())
            .timeout(Duration::from_secs(2));
        let output = LocalProcessRunner::new().capture(&request).unwrap();
        assert!(
            output.success,
            "argument {value:?}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = value
            .replace("\r\n", "\u{2028}")
            .replace(['\r', '\n'], "\u{2028}");
        assert_eq!(output.stdout, expected.as_bytes(), "argument {value:?}");
        assert!(!directory.path().join("neomax-injected-marker").exists());
    }
}

#[cfg(windows)]
#[test]
fn windows_batch_processes_start_a_spaced_path_without_escaped_quotes() {
    let directory = tempfile::tempdir().unwrap();
    let script_directory = directory.path().join("cmd fixture");
    std::fs::create_dir_all(&script_directory).unwrap();
    let script = script_directory.join("fake provider.cmd");
    std::fs::write(&script, b"@echo off\r\necho(started\r\n").unwrap();
    let request = ProcessRequest::new(script)
        .cwd(directory.path())
        .timeout(Duration::from_secs(2));
    let output = LocalProcessRunner::new().capture(&request).unwrap();
    assert!(
        output.success,
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "started");
}

#[test]
fn child_process_fixture() {
    match std::env::var("NEOMAX_BOUNDED_IO_CHILD").as_deref() {
        Ok("large") => {
            print!("{}", "x".repeat(1024));
        }
        Ok("stderr") => {
            eprint!("{}", "x".repeat(1024));
        }
        Ok("hang") => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        Ok("fail") => std::process::exit(23),
        _ => {}
    }
}
