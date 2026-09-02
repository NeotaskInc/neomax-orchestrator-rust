#![cfg(windows)]

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;

use crate::{Error, Result};

pub(crate) fn is_current_executable(path: &Path) -> bool {
    let Ok(current) = env::current_exe() else {
        return false;
    };
    fs::canonicalize(path).ok() == fs::canonicalize(current).ok()
}

pub(crate) fn defer_delete(path: &Path) -> Result<()> {
    let escaped = crate::runtime::quote_cmd_argument(path.as_os_str())?;
    let contents = format!(
        "@echo off\r\n@setlocal EnableExtensions DisableDelayedExpansion\r\n:wait\r\ndel /f /q {escaped} >nul 2>&1\r\nif exist {escaped} (\r\n  timeout /t 1 /nobreak >nul\r\n  goto wait\r\n)\r\ndel /f /q \"%~f0\" >nul 2>&1\r\n"
    );
    let mut script = tempfile::Builder::new()
        .prefix("neomax-remove-")
        .suffix(".cmd")
        .tempfile_in(env::temp_dir())?;
    script.write_all(contents.as_bytes())?;
    script.flush()?;
    let (script_file, script) = script.keep().map_err(|error| error.error)?;
    drop(script_file);
    let mut command = crate::runtime::RuntimeEnvironment::process()
        .process_command(
            &script,
            std::iter::empty::<std::ffi::OsString>(),
            script.parent().unwrap_or_else(|| Path::new(".")),
        )
        .map_err(|error| Error::Message(format!("could not schedule removal: {error}")))?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            Error::Message(format!(
                "could not schedule removal of {}: {error}",
                path.display()
            ))
        })
}
