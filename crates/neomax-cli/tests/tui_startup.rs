#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

#[test]
#[ignore = "manual before/after startup and memory measurement"]
fn large_history_startup_remains_interactive_and_loads_every_session() {
    let temp = tempfile::tempdir().unwrap();
    let sessions = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let security = bin.join("security");
    std::fs::write(&security, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&security, std::fs::Permissions::from_mode(0o755)).unwrap();
    let padding = "x".repeat(2 * 1024 * 1024);
    for index in 0..16 {
        let content = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({"type":"session_meta","payload":{"id":format!("fixture-{index}"),"cwd":temp.path(),"model":"gpt-6-astra"}}),
            serde_json::json!({"type":"response_item","payload":{"type":"message","content":[{"type":"output_text","text":padding}]}}),
            serde_json::json!({"usage":{"input_tokens":1000,"output_tokens":200,"total_tokens":1200}}),
        );
        std::fs::write(sessions.join(format!("rollout-{index}.jsonl")), content).unwrap();
    }
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 36,
            cols: 125,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let executable = std::env::var_os("NEOMAX_TUI_BENCH_BIN")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_neomax").into());
    let mut command = CommandBuilder::new(executable);
    command.arg("tui");
    command.cwd(temp.path());
    command.env_clear();
    command.env("HOME", temp.path());
    command.env("USERPROFILE", temp.path());
    command.env("NEOMAX_HOME", temp.path().join("state"));
    command.env("NEOMAX_INVOKED_AS", "neomax");
    command.env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    command.env("TERM", "xterm-256color");
    let started = Instant::now();
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let pid = child.process_id().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = [0; 8192];
        while let Ok(count) = reader.read(&mut bytes) {
            if count == 0 || sender.send(bytes[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut screen = vt100::Parser::new(36, 125, 0);
    let mut first_snapshot = None;
    let mut navigation = None;
    let mut navigation_requested = false;
    let mut peak_rss_kib = 0;
    let mut complete = false;
    while started.elapsed() < Duration::from_secs(45) {
        if let Ok(bytes) = receiver.recv_timeout(Duration::from_millis(50)) {
            screen.process(&bytes);
        }
        let contents = screen.screen().contents();
        if !navigation_requested
            && (contents.contains("Agents") || contents.contains("Launch Neomax"))
        {
            writer
                .write_all(if contents.contains("Launch Neomax") {
                    b"\x1b[C"
                } else {
                    b"4"
                })
                .unwrap();
            writer.flush().unwrap();
            navigation_requested = true;
        }
        if navigation.is_none()
            && (contents.contains("New orchestrator") || contents.contains("Chat with your Neomax"))
        {
            navigation = Some(started.elapsed().as_millis());
            writer
                .write_all(if contents.contains("Chat with your Neomax") {
                    b"\x1b[C"
                } else {
                    b"1"
                })
                .unwrap();
            writer.flush().unwrap();
        }
        if first_snapshot.is_none() && contents.contains("updated") {
            first_snapshot = Some(started.elapsed().as_millis());
        }
        if let Ok(output) = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
        {
            peak_rss_kib = peak_rss_kib.max(
                String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u64>()
                    .unwrap_or_default(),
            );
        }
        if (contents.contains("Agents · 16") && contents.contains("r refresh"))
            || (contents.contains("Sessions · 7d · 16") && contents.contains("c chat"))
        {
            complete = true;
            break;
        }
    }
    let completion_ms = started.elapsed().as_millis();
    writer.write_all(b"q").unwrap();
    writer.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    if child.try_wait().unwrap().is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    println!(
        "TUI_STARTUP {}",
        serde_json::json!({"fixture_bytes":32*1024*1024,"sessions":16,"first_snapshot_ms":first_snapshot,"navigation_ms":navigation,"completion_ms":completion_ms,"peak_rss_kib":peak_rss_kib,"complete":complete})
    );
    assert!(
        complete,
        "not all sessions reached the fleet: {}",
        screen.screen().contents()
    );
    assert!(
        navigation.is_some(),
        "navigation did not respond while loading"
    );
    assert!(first_snapshot.is_some());
}
