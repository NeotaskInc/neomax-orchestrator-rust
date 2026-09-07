use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc;

use anyhow::{Result, anyhow};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

pub struct Session {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    output: mpsc::Receiver<Vec<u8>>,
    pub parser: vt100::Parser,
    pub exited: Option<String>,
    pub input: bool,
    pub scroll: usize,
    query: Vec<u8>,
}

impl Session {
    pub fn spawn(
        executable: &Path,
        cwd: &Path,
        args: &[String],
        rows: u16,
        cols: u16,
    ) -> Result<Self> {
        let pair = native_pty_system().openpty(size(rows, cols))?;
        let mut command = CommandBuilder::new(executable);
        command.args(args);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let (sender, output) = mpsc::sync_channel(64);
        std::thread::spawn(move || {
            let mut buffer = [0; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if sender.send(buffer[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Self {
            master: pair.master,
            child,
            writer,
            output,
            parser: vt100::Parser::new(rows.max(1), cols.max(1), 2000),
            exited: None,
            input: true,
            scroll: 0,
            query: Vec::new(),
        })
    }

    pub fn poll(&mut self) -> Result<()> {
        let output = self.output.try_iter().take(64).collect::<Vec<_>>();
        for bytes in output {
            let mut replies = Vec::new();
            for byte in bytes {
                self.parser.process(&[byte]);
                if byte == 27 {
                    self.query.clear();
                }
                if self.query.len() < 32 {
                    self.query.push(byte);
                }
                if let Some(reply) = query_reply(&self.query, self.parser.screen()) {
                    if replies.len() < 4096 {
                        replies.extend(reply);
                    }
                    self.query.clear();
                }
            }
            if !replies.is_empty() && self.exited.is_none() {
                self.writer.write_all(&replies)?;
                self.writer.flush()?;
            }
        }
        if self.exited.is_none() {
            if let Some(status) = self.child.try_wait()? {
                self.exited = Some(status.to_string());
                self.input = false;
            }
        }
        Ok(())
    }

    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        let new = size(rows, cols);
        if self.parser.screen().size() != (new.rows, new.cols) {
            self.master.resize(new)?;
            self.parser.set_size(new.rows, new.cols);
        }
        Ok(())
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<()> {
        if self.exited.is_some() {
            return Err(anyhow!("The orchestrator has exited"));
        }
        self.scroll = 0;
        self.parser.set_scrollback(0);
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn paste(&mut self, value: &str) -> Result<()> {
        let value = value.replace('\u{1b}', "");
        if self.parser.screen().bracketed_paste() {
            self.send(format!("\u{1b}[200~{value}\u{1b}[201~").as_bytes())
        } else {
            self.send(value.as_bytes())
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        if self.exited.is_none() {
            if let Some(status) = self.child.try_wait()? {
                self.exited = Some(status.to_string());
                return Ok(());
            }
            #[cfg(unix)]
            if let Some(child) = (self.child.as_mut() as &mut dyn portable_pty::Child)
                .downcast_mut::<std::process::Child>()
            {
                neomax_core::io::process_group::terminate_detached(child)?;
                self.exited = Some(self.child.wait()?.to_string());
                return Ok(());
            }
            #[cfg(windows)]
            if let Some(pid) = self.child.process_id() {
                neomax_core::io::process_group::terminate_worker(pid)?;
            }
            self.child.kill()?;
            self.exited = Some(self.child.wait()?.to_string());
        }
        Ok(())
    }
}

fn query_reply(query: &[u8], screen: &vt100::Screen) -> Option<Vec<u8>> {
    let reply = match query {
        b"\x1b[5n" => "\x1b[0n".to_string(),
        b"\x1b[6n" => {
            let (row, col) = screen.cursor_position();
            format!("\x1b[{};{}R", row + 1, col + 1)
        }
        b"\x1b[c" | b"\x1b[0c" => "\x1b[?1;2c".to_string(),
        b"\x1b[>c" | b"\x1b[>0c" => "\x1b[>0;0;0c".to_string(),
        b"\x1b[?u" => "\x1b[?0u".to_string(),
        b"\x1b[18t" => {
            let (rows, cols) = screen.size();
            format!("\x1b[8;{rows};{cols}t")
        }
        _ => return None,
    };
    Some(reply.into_bytes())
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn size(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows: rows.clamp(1, 500),
        cols: cols.clamp(1, 1000),
        pixel_width: 0,
        pixel_height: 0,
    }
}

pub fn key_bytes(key: KeyEvent, application_cursor: bool) -> Vec<u8> {
    let mut bytes = match key.code {
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let c = c.to_ascii_lowercase();
            match c {
                'a'..='z' => vec![c as u8 - b'a' + 1],
                ' ' | '@' => vec![0],
                '[' => vec![27],
                '\\' | '4' => vec![28],
                ']' | '5' => vec![29],
                '^' | '6' => vec![30],
                '_' | '7' => vec![31],
                _ => Vec::new(),
            }
        }
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![127],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![27],
        KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
            let suffix = match key.code {
                KeyCode::Up => 'A',
                KeyCode::Down => 'B',
                KeyCode::Right => 'C',
                _ => 'D',
            };
            format!(
                "\u{1b}{}{suffix}",
                if application_cursor { 'O' } else { '[' }
            )
            .into_bytes()
        }
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::F(n @ 1..=4) => vec![27, b'O', b'P' + n - 1],
        KeyCode::F(n @ 5..=12) => format!(
            "\u{1b}[{}~",
            [15, 17, 18, 19, 20, 21, 23, 24][usize::from(n - 5)]
        )
        .into_bytes(),
        _ => Vec::new(),
    };
    if key.modifiers.contains(KeyModifiers::ALT) && !bytes.is_empty() {
        bytes.insert(0, 27);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_preserves_unicode_controls_and_cursor_mode() {
        assert_eq!(
            key_bytes(KeyEvent::new(KeyCode::Char('λ'), KeyModifiers::NONE), false),
            "λ".as_bytes()
        );
        assert_eq!(
            key_bytes(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                false
            ),
            [3]
        );
        assert_eq!(
            key_bytes(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), true),
            b"\x1bOA"
        );
    }

    #[test]
    fn startup_queries_receive_virtual_terminal_coordinates() {
        let mut parser = vt100::Parser::new(10, 40, 0);
        parser.process(b"hello");
        assert_eq!(
            query_reply(b"\x1b[6n", parser.screen()).unwrap(),
            b"\x1b[1;6R"
        );
        assert_eq!(
            query_reply(b"\x1b[18t", parser.screen()).unwrap(),
            b"\x1b[8;10;40t"
        );
        assert!(query_reply(b"\x1b]52;c;payload", parser.screen()).is_none());
    }

    #[test]
    fn terminal_parser_keeps_escape_side_effects_inside_virtual_screen() {
        let mut parser = vt100::Parser::new(4, 20, 8);
        parser.process(b"\x1b]52;c;secret\x07hello\r\nworld");
        assert_eq!(parser.screen().contents(), "hello\nworld");
    }

    #[cfg(unix)]
    #[test]
    fn owned_pty_accepts_input_and_reports_exit_without_a_provider() {
        let temp = tempfile::tempdir().unwrap();
        let mut session = Session::spawn(
            Path::new("/bin/sh"),
            temp.path(),
            &[
                "-c".into(),
                "read answer; printf 'received:%s' \"$answer\"".into(),
            ],
            8,
            40,
        )
        .unwrap();
        session.send(b"fixture\r").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            session.poll().unwrap();
            if session.exited.is_some()
                && session
                    .parser
                    .screen()
                    .contents()
                    .contains("received:fixture")
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("fake terminal did not complete");
    }
}
