#![cfg(unix)]

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

#[test]
fn tui_launch_cancel_prompt_page_switch_resize_and_confirmed_exit_are_hermetic() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    let profile = temp.path().join(".codex");
    std::fs::create_dir_all(&bin).unwrap();
    let security = bin.join("security");
    std::fs::write(&security, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&security, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("auth.json"),
        r#"{"account_id":"fixture-account","email":"fixture@example.test","tokens":{"access_token":"fixture-token","refresh_token":"fixture-refresh"}}"#,
    )
    .unwrap();
    let provider = bin.join("codex");
    std::fs::write(
        &provider,
        r##"#!/bin/sh
if [ "$1" = "--version" ]; then printf 'codex-cli fixture\n'; exit 0; fi
printf '%s\n' "$@" > "$(dirname "$0")/launch-args"
printf 'FIXTURE ORCHESTRATOR READY\n'
while IFS= read -r line; do printf 'FIXTURE REPLY: %s\n' "$line"; done
"##,
    )
    .unwrap();
    std::fs::set_permissions(&provider, std::fs::Permissions::from_mode(0o755)).unwrap();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 36,
            cols: 125,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_neomax"));
    command.cwd(temp.path());
    command.env_clear();
    command.env("HOME", temp.path());
    command.env("USERPROFILE", temp.path());
    command.env("NEOMAX_HOME", temp.path().join("state"));
    command.env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    command.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    let (sender, receiver) = mpsc::sync_channel(128);
    std::thread::spawn(move || {
        let mut buffer = [0; 8192];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut screen = vt100::Parser::new(36, 125, 0);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_for(&receiver, &mut screen, "Launch Neomax");
        send(&mut writer, b"\x1b[B\r\x1b[C\x1b[C\r");
        wait_for(&receiver, &mut screen, "codex");
        send(&mut writer, b"\x1b[A\r");
        wait_for(&receiver, &mut screen, "Start orchestrator?");
        send(&mut writer, b"n");
        wait_for(&receiver, &mut screen, "Launch Neomax");
        assert!(!bin.join("launch-args").exists());
        send(&mut writer, b"\r");
        wait_for(&receiver, &mut screen, "Start orchestrator?");
        send(&mut writer, b"\r");
        wait_for(&receiver, &mut screen, "FIXTURE ORCHESTRATOR READY");
        send(&mut writer, b"hello fixture\r");
        wait_for(&receiver, &mut screen, "FIXTURE REPLY: hello fixture");
        send(&mut writer, b"\x1d\x1b[C");
        wait_for(&receiver, &mut screen, "Selected session");
        send(&mut writer, b"\x1b[C\x1b[C");
        wait_for(&receiver, &mut screen, "Account details");
        send(&mut writer, b"]]");
        wait_for(&receiver, &mut screen, "fixture@example.test");
        send(&mut writer, b"\r");
        wait_for(&receiver, &mut screen, "Profile directory");
        send(&mut writer, b"\x1b");
        send(&mut writer, b"l");
        wait_for(&receiver, &mut screen, "Launch Neomax");
        send(&mut writer, b"\x1b[B\x1b[B\r");
        wait_for(&receiver, &mut screen, "Codex speed");
        wait_for(&receiver, &mut screen, "fixture@example.test");
        pair.master
            .resize(PtySize {
                rows: 26,
                cols: 90,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        screen.set_size(26, 90);
        send(&mut writer, b"c");
        wait_for(&receiver, &mut screen, "Enter focuses");
        send(&mut writer, b"q");
        wait_for(&receiver, &mut screen, "Stop orchestrator and quit?");
        send(&mut writer, b"n");
        wait_for(&receiver, &mut screen, "Enter focuses");
        send(&mut writer, b"q");
        wait_for(&receiver, &mut screen, "Stop orchestrator and quit?");
        send(&mut writer, b"y");
        let deadline = Instant::now() + Duration::from_secs(8);
        while child.try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "TUI failed to stop its owned orchestrator"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let args = std::fs::read_to_string(bin.join("launch-args")).unwrap();
        assert!(args.contains("service_tier=default"));
        assert!(!args.contains("service_tier=fast"));
    }));
    if child.try_wait().unwrap().is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
}

fn send(writer: &mut dyn Write, bytes: &[u8]) {
    writer.write_all(bytes).unwrap();
    writer.flush().unwrap();
}

fn wait_for(receiver: &mpsc::Receiver<Vec<u8>>, parser: &mut vt100::Parser, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(bytes) => parser.process(&bytes),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!(
                "terminal closed before {needle}: {}",
                parser.screen().contents()
            ),
        }
        if parser.screen().contents().contains(needle) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "missing {needle}: {}",
            parser.screen().contents()
        );
    }
}
