use std::fs;
#[cfg(windows)]
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use neomax_core::io::path_to_string;

use crate::config::AgentConfig;
use crate::install::runner::{COMMAND_TIMEOUT, CommandRunner, success};
use crate::install::{
    ServiceReport, ServiceState, ensure_parent, validate_service_paths, windows_environment_values,
    write_service_artifact,
};

#[cfg(target_os = "windows")]
use crate::install::runner::SystemRunner;

pub(crate) const TASK_NAME: &str = r"Neomax\UsageAgent";
const TASK_URI: &str = r"\Neomax\UsageAgent";
#[cfg(not(windows))]
const TASK_RUNNER: &str = "schtasks.exe";
const MAX_CMD_ARGUMENTS: usize = 8_000;
#[cfg(windows)]
const SYSTEM_DIRECTORY_BUFFER_LEN: usize = 32_768;

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
}

#[cfg(target_os = "windows")]
pub(crate) fn install(config: &AgentConfig) -> Result<ServiceReport> {
    install_with(config, &SystemRunner)
}

pub(crate) fn install_with(
    config: &AgentConfig,
    runner: &dyn CommandRunner,
) -> Result<ServiceReport> {
    validate_service_paths(config)?;
    let task_runner = task_runner()?;
    let path = &config.paths.windows_task_xml;
    let path_text = path_to_string("Windows task definition path", path)?;
    ensure_parent(path)?;
    let _state_guard = neomax_core::io::PathGuard::ensure_directory(&config.paths.state.state)?;
    write_service_artifact(path, &task_xml(config)?)
        .with_context(|| format!("write Windows task definition {}", path.display()))?;
    let create = runner.run(
        &task_runner,
        &["/Create", "/TN", TASK_NAME, "/XML", &path_text, "/F"],
        COMMAND_TIMEOUT,
    );
    let run = if create.as_ref().is_ok_and(success) {
        runner.run(&task_runner, &["/Run", "/TN", TASK_NAME], COMMAND_TIMEOUT)
    } else {
        Err(anyhow::anyhow!("task creation failed"))
    };
    let active = create.as_ref().is_ok_and(success) && run.as_ref().is_ok_and(success);
    Ok(ServiceReport {
        platform: "windows".into(),
        path: path_text,
        state: if active {
            ServiceState::Active
        } else {
            ServiceState::Unknown
        },
        detail: if active {
            "installed and started"
        } else {
            "wrote task definition but Task Scheduler could not start it"
        }
        .into(),
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn uninstall(config: &AgentConfig) -> Result<ServiceReport> {
    uninstall_with(config, &SystemRunner)
}

pub(crate) fn uninstall_with(
    config: &AgentConfig,
    runner: &dyn CommandRunner,
) -> Result<ServiceReport> {
    validate_service_paths(config)?;
    let task_runner = task_runner()?;
    let _ = runner.run(&task_runner, &["/End", "/TN", TASK_NAME], COMMAND_TIMEOUT);
    let _ = runner.run(
        &task_runner,
        &["/Delete", "/TN", TASK_NAME, "/F"],
        COMMAND_TIMEOUT,
    );
    let path = &config.paths.windows_task_xml;
    let path_text = path_to_string("Windows task definition path", path)?;
    let _path_guard = match neomax_core::io::PathGuard::for_existing_parent(path) {
        Ok(guard) => Some(guard),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    match fs::symlink_metadata(path) {
        Ok(metadata) if super::is_link_like(&metadata) || !metadata.file_type().is_file() => {
            anyhow::bail!("refusing to remove an unsafe Windows task definition")
        }
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(ServiceReport {
        platform: "windows".into(),
        path: path_text,
        state: ServiceState::Inactive,
        detail: "uninstalled".into(),
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn status(config: &AgentConfig) -> Result<ServiceReport> {
    status_with(config, &SystemRunner)
}

pub(crate) fn status_with(
    config: &AgentConfig,
    runner: &dyn CommandRunner,
) -> Result<ServiceReport> {
    validate_service_paths(config)?;
    let task_runner = task_runner()?;
    let path_text = path_to_string(
        "Windows task definition path",
        &config.paths.windows_task_xml,
    )?;
    let output = runner.run(
        &task_runner,
        &["/Query", "/TN", TASK_NAME, "/FO", "CSV", "/NH", "/V"],
        COMMAND_TIMEOUT,
    );
    let exists = output.as_ref().is_ok_and(success);
    let running = output
        .as_ref()
        .is_ok_and(|output| task_output_is_running(&output.stdout));
    Ok(ServiceReport {
        platform: "windows".into(),
        path: path_text,
        state: if running {
            ServiceState::Active
        } else if exists {
            ServiceState::Loaded
        } else {
            ServiceState::Inactive
        },
        detail: if running {
            "running"
        } else if exists {
            "registered but not running"
        } else {
            "not registered; run install"
        }
        .into(),
    })
}

fn task_xml(config: &AgentConfig) -> anyhow::Result<String> {
    validate_service_paths(config)?;
    let shell = xml_escape(&task_shell()?)?;
    let state = xml_escape(&path_to_string(
        "usage-agent state path",
        &config.paths.state.state,
    )?)?;
    let executable = path_to_string("usage-agent executable path", &config.executable)?;
    let arguments = xml_escape(&command_arguments(config, &executable)?)?;
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Task version=\"1.4\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\n  <RegistrationInfo><URI>{TASK_URI}</URI><Description>Neomax local usage collector</Description></RegistrationInfo>\n  <Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>\n  <Principals><Principal id=\"Author\"><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>\n  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><AllowHardTerminate>true</AllowHardTerminate><StartWhenAvailable>true</StartWhenAvailable><RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><Enabled>true</Enabled></Settings>\n  <Actions Context=\"Author\"><Exec><Command>{shell}</Command><Arguments>{arguments}</Arguments><WorkingDirectory>{state}</WorkingDirectory></Exec></Actions>\n</Task>\n"
    ))
}

fn task_runner() -> anyhow::Result<String> {
    #[cfg(windows)]
    {
        return system_binary("schtasks.exe");
    }
    #[cfg(not(windows))]
    {
        Ok(TASK_RUNNER.into())
    }
}

fn task_shell() -> anyhow::Result<String> {
    #[cfg(windows)]
    {
        return system_binary("cmd.exe");
    }
    #[cfg(not(windows))]
    {
        Ok("cmd.exe".into())
    }
}

#[cfg(windows)]
fn system_binary(name: &str) -> anyhow::Result<String> {
    if name.is_empty() || Path::new(name).components().count() != 1 {
        anyhow::bail!("Windows system executable name is invalid");
    }
    let system32 = system_directory()?;
    ensure_directory(&system32)?;
    let executable = system32.join(name);
    let metadata = fs::symlink_metadata(&executable)?;
    let linked = has_linked_component(&executable)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || is_reparse_point(&metadata)
        || linked
    {
        anyhow::bail!("Windows system executable is not a trusted regular file");
    }
    Ok(path_to_string("Windows system executable", &executable)?)
}

#[cfg(windows)]
fn system_directory() -> anyhow::Result<PathBuf> {
    let mut buffer = [0u16; SYSTEM_DIRECTORY_BUFFER_LEN];
    // SAFETY: the buffer is writable UTF-16 storage and its bounded length is
    // provided to the Win32 API in the required element count.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    system_directory_from_utf16(&buffer, length)
}

#[cfg(windows)]
fn system_directory_from_utf16(buffer: &[u16], length: u32) -> anyhow::Result<PathBuf> {
    let length = usize::try_from(length)
        .map_err(|_| anyhow::anyhow!("Windows system directory length is invalid"))?;
    if length == 0 || length >= buffer.len() {
        anyhow::bail!("Windows system directory output exceeds its bounded buffer");
    }
    let units = &buffer[..length];
    if units.contains(&0) {
        anyhow::bail!("Windows system directory contains an embedded NUL");
    }
    let text = String::from_utf16(units)
        .map_err(|_| anyhow::anyhow!("Windows system directory is not valid UTF-16"))?;
    if text.chars().any(char::is_control) {
        anyhow::bail!("Windows system directory contains control characters");
    }
    let path = PathBuf::from(text);
    if !is_drive_absolute_windows(&path) || has_unsafe_component(&path) {
        anyhow::bail!("Windows system directory must be an absolute normalized drive path");
    }
    Ok(path)
}

#[cfg(windows)]
fn ensure_directory(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let linked = has_linked_component(path)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || is_reparse_point(&metadata)
        || linked
    {
        anyhow::bail!("Windows system directory is not trusted");
    }
    Ok(())
}

#[cfg(windows)]
fn is_drive_absolute_windows(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.as_bytes().get(1) == Some(&b':')
        && text
            .as_bytes()
            .get(2)
            .is_some_and(|separator| *separator == b'/' || *separator == b'\\')
}

#[cfg(windows)]
fn has_unsafe_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes() & 0x400 != 0
}

#[cfg(windows)]
fn has_linked_component(path: &Path) -> anyhow::Result<bool> {
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn task_output_is_running(stdout: &[u8]) -> bool {
    let text = String::from_utf8_lossy(stdout);
    text.lines().flat_map(csv_fields).any(|field| {
        let value = field.trim().trim_matches('"');
        value.eq_ignore_ascii_case("status: running")
            || value.eq_ignore_ascii_case("running")
            || value
                .strip_prefix("0x")
                .and_then(|digits| u32::from_str_radix(digits, 16).ok())
                .is_some_and(|result| result == 0x41301)
            || value.parse::<u32>().ok() == Some(0x41301)
    })
}

fn csv_fields(record: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = record.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(std::mem::take(&mut field));
            }
            character => field.push(character),
        }
    }
    fields.push(field);
    fields
}

fn command_arguments(config: &AgentConfig, executable: &str) -> anyhow::Result<String> {
    let assignments = windows_environment_values(config)?
        .into_iter()
        .map(|(name, value)| {
            let value = cmd_escape(&value)?;
            Ok(format!("set \"{name}={value}\""))
        })
        .collect::<anyhow::Result<Vec<_>>>()?
        .join(" && ");
    let executable = cmd_escape(executable)?;
    let command = format!("/d /e:on /v:off /s /c \"{assignments} && \"{executable}\" run\"");
    if command.len() > MAX_CMD_ARGUMENTS {
        return Err(anyhow::anyhow!(
            "Windows usage-agent task arguments exceed the cmd.exe limit"
        ));
    }
    Ok(command)
}

fn cmd_escape(value: &str) -> anyhow::Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    if value.chars().any(|character| character.is_control()) {
        return Err(anyhow::anyhow!(
            "Windows task values must not contain control characters"
        ));
    }
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '!' => escaped.push_str("^!"),
            '^' => escaped.push_str("^^"),
            '&' => escaped.push_str("^&"),
            '|' => escaped.push_str("^|"),
            '<' => escaped.push_str("^<"),
            '>' => escaped.push_str("^>"),
            '(' => escaped.push_str("^("),
            ')' => escaped.push_str("^)"),
            '"' => escaped.push_str("^\""),
            '%' => {
                // Expand an empty substring so cmd.exe keeps the original
                // percent literal instead of looking up an environment value.
                escaped.push_str("%%cd:~,%");
                escaped.push('%');
            }
            character => escaped.push(character),
        }
    }
    Ok(escaped)
}

#[cfg(test)]
fn cmd_unescape(value: &str) -> String {
    let mut unescaped = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character == '^' {
            if let Some(next) = chars.next() {
                unescaped.push(next);
            } else {
                unescaped.push('^');
            }
        } else {
            unescaped.push(character);
        }
    }
    unescaped
}

fn xml_escape(value: &str) -> anyhow::Result<String> {
    if value.chars().any(|character| character.is_control()) {
        return Err(anyhow::anyhow!(
            "Windows task values must not contain control characters"
        ));
    }
    Ok(value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;"))
}

#[cfg(test)]
mod tests;
