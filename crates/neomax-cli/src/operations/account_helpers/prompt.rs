use std::io::{self, Write};

use anyhow::{Context, Result, bail};

pub(crate) trait PromptPort: Send + Sync {
    fn selection(&self, prompt: &str) -> Result<String>;
    fn secret(&self, prompt: &str) -> Result<String>;
}

pub(crate) struct TerminalPrompt;

pub(crate) static TERMINAL_PROMPT: TerminalPrompt = TerminalPrompt;

impl PromptPort for TerminalPrompt {
    fn selection(&self, prompt: &str) -> Result<String> {
        eprint!("{prompt}");
        io::stderr()
            .flush()
            .context("could not flush auth prompt")?;
        let mut value = String::new();
        io::stdin()
            .read_line(&mut value)
            .context("could not read auth selection")?;
        Ok(value)
    }

    fn secret(&self, prompt: &str) -> Result<String> {
        read_secret(prompt)
    }
}

#[cfg(unix)]
fn read_secret(prompt: &str) -> Result<String> {
    use std::os::fd::AsRawFd;

    eprint!("{prompt}");
    io::stderr()
        .flush()
        .context("could not flush secret prompt")?;
    let stdin = io::stdin();
    let fd = stdin.as_raw_fd();
    if unsafe { libc::isatty(fd) } != 1 {
        let mut value = String::new();
        stdin
            .read_line(&mut value)
            .context("could not read API key")?;
        return Ok(trim_secret_line(&value));
    }

    let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
    if unsafe { libc::tcgetattr(fd, &mut original) } != 0 {
        bail!("could not prepare the terminal for secret input")
    }
    let mut hidden = original;
    hidden.c_lflag &= !libc::ECHO;
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &hidden) } != 0 {
        bail!("could not disable terminal echo for secret input")
    }

    let mut value = String::new();
    let read_result = stdin
        .read_line(&mut value)
        .context("could not read API key");
    let restore_result = unsafe { libc::tcsetattr(fd, libc::TCSANOW, &original) };
    println!();
    if restore_result != 0 {
        bail!("could not restore terminal echo after secret input")
    }
    read_result?;
    Ok(trim_secret_line(&value))
}

#[cfg(windows)]
fn read_secret(prompt: &str) -> Result<String> {
    use std::os::windows::io::AsRawHandle;

    eprint!("{prompt}");
    io::stderr()
        .flush()
        .context("could not flush secret prompt")?;
    let stdin = io::stdin();
    let handle = stdin.as_raw_handle();
    let mut original_mode = 0_u32;
    if unsafe { windows_console::get_console_mode(handle, &mut original_mode) } == 0 {
        bail!(
            "API-key prompt requires an interactive Windows console; set the provider API-key environment variable"
        )
    }
    let hidden_mode = windows_console::without_echo(original_mode);
    if unsafe { windows_console::set_console_mode(handle, hidden_mode) } == 0 {
        bail!("could not disable Windows console echo for secret input")
    }

    let mut value = String::new();
    let read_result = stdin
        .read_line(&mut value)
        .context("could not read API key");
    let restore_result = unsafe { windows_console::set_console_mode(handle, original_mode) };
    println!();
    if restore_result == 0 {
        bail!("could not restore Windows console echo after secret input")
    }
    read_result?;
    Ok(trim_secret_line(&value))
}

fn trim_secret_line(value: &str) -> String {
    value.trim_end_matches(['\r', '\n']).to_owned()
}

#[cfg(windows)]
mod windows_console {
    use std::ffi::c_void;

    const ENABLE_ECHO_INPUT: u32 = 0x0004;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetConsoleMode"]
        pub(super) fn get_console_mode(handle: *mut c_void, mode: *mut u32) -> i32;
        #[link_name = "SetConsoleMode"]
        pub(super) fn set_console_mode(handle: *mut c_void, mode: u32) -> i32;
    }

    pub(super) fn without_echo(mode: u32) -> u32 {
        mode & !ENABLE_ECHO_INPUT
    }

    #[cfg(test)]
    mod tests {
        use super::{ENABLE_ECHO_INPUT, without_echo};

        #[test]
        fn windows_secret_mode_clears_only_console_echo() {
            let original = u32::MAX;
            let hidden = without_echo(original);
            assert_eq!(hidden, original & !ENABLE_ECHO_INPUT);
            assert_eq!(hidden & ENABLE_ECHO_INPUT, 0);
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn read_secret(_prompt: &str) -> Result<String> {
    bail!(
        "API-key prompts are unavailable on this platform; set the provider API-key environment variable"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixturePrompt {
        selection: String,
        secret: String,
    }

    impl PromptPort for FixturePrompt {
        fn selection(&self, _prompt: &str) -> Result<String> {
            Ok(self.selection.clone())
        }

        fn secret(&self, _prompt: &str) -> Result<String> {
            Ok(self.secret.clone())
        }
    }

    #[test]
    fn injected_prompt_returns_values_without_rendering_or_echoing_them() -> Result<()> {
        let prompt = FixturePrompt {
            selection: "3".into(),
            secret: "fixture-secret".into(),
        };
        assert_eq!(prompt.selection("ignored")?, "3");
        assert_eq!(prompt.secret("ignored")?, "fixture-secret");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unix_secret_fixture_preserves_secret_bytes_without_terminal_output() {
        let secret = " fixture-secret ";
        assert_eq!(super::trim_secret_line(&format!("{secret}\r\n")), secret);
    }
}
