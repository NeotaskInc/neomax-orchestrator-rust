use std::mem::size_of;

use windows_sys::Win32::Foundation::ERROR_BAD_LENGTH;
use windows_sys::Win32::Security::TOKEN_QUERY;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_VM_READ,
};

use crate::{Error, Result};

use super::handles::OwnedHandle;
use super::parsing::{is_claude_process, recognized_claude_image};
use super::remote::{process_command_line, process_environment_value, process_image_path};
use super::security::{current_user_sid, token_user_sid};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsProcessInfo {
    pub(crate) pid: u32,
    pub(crate) parent_pid: Option<u32>,
    pub(crate) image_path: String,
    pub(crate) command_line: Option<String>,
    pub(crate) config_dir: Option<String>,
}

pub(crate) trait WindowsProcessInspector: Send + Sync {
    fn processes(&self) -> Result<Vec<WindowsProcessInfo>>;
}

#[derive(Debug, Default)]
pub(crate) struct NativeWindowsProcessInspector;

impl WindowsProcessInspector for NativeWindowsProcessInspector {
    fn processes(&self) -> Result<Vec<WindowsProcessInfo>> {
        let current_sid = current_user_sid()
            .ok_or_else(|| Error::Message("unable to identify the current Windows user".into()))?;
        let snapshot = process_snapshot()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let first = unsafe {
            // SAFETY: entry points to a writable value with the documented dwSize.
            Process32FirstW(snapshot.raw(), &mut entry)
        };
        if first == 0 {
            return Err(Error::Io(std::io::Error::last_os_error()));
        }

        let mut processes = Vec::new();
        loop {
            if let Some(process) = inspect_process(&entry, &current_sid) {
                processes.push(process);
            }
            let next = unsafe {
                // SAFETY: entry remains a valid writable PROCESSENTRY32W for this snapshot.
                Process32NextW(snapshot.raw(), &mut entry)
            };
            if next == 0 {
                break;
            }
        }
        Ok(processes)
    }
}

const MAX_SNAPSHOT_ATTEMPTS: usize = 4;

fn process_snapshot() -> Result<OwnedHandle> {
    for attempt in 0..MAX_SNAPSHOT_ATTEMPTS {
        let snapshot = unsafe {
            // SAFETY: the snapshot has no caller-owned pointers and is closed by OwnedHandle.
            CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
        };
        if let Some(snapshot) = OwnedHandle::new(snapshot) {
            return Ok(snapshot);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_BAD_LENGTH as i32)
            || attempt + 1 == MAX_SNAPSHOT_ATTEMPTS
        {
            return Err(Error::Io(error));
        }
        std::thread::yield_now();
    }
    unreachable!("the bounded snapshot loop always returns")
}

fn inspect_process(entry: &PROCESSENTRY32W, current_sid: &[u8]) -> Option<WindowsProcessInfo> {
    if entry.th32ProcessID == 0 {
        return None;
    }
    let process = unsafe {
        // SAFETY: the process id came from the system snapshot and the handle is owned below.
        OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            entry.th32ProcessID,
        )
    };
    let process = OwnedHandle::new(process)?;
    let token = unsafe {
        // SAFETY: process is a live owned handle and token is an out pointer we initialize.
        let mut token = std::ptr::null_mut();
        (OpenProcessToken(process.raw(), TOKEN_QUERY, &mut token) != 0).then_some(token)
    }?;
    let token = OwnedHandle::new(token)?;
    if token_user_sid(token.raw()).as_deref() != Some(current_sid) {
        return None;
    }
    let image_path = process_image_path(process.raw())?;
    if !recognized_claude_image(&image_path) {
        return None;
    }
    let command_line = process_command_line(process.raw());
    let mut info = WindowsProcessInfo {
        pid: entry.th32ProcessID,
        parent_pid: (entry.th32ParentProcessID != 0).then_some(entry.th32ParentProcessID),
        image_path,
        command_line,
        config_dir: None,
    };
    if !is_claude_process(&info) {
        return None;
    }
    info.config_dir = process_config_dir(entry.th32ProcessID);
    Some(info)
}

fn process_config_dir(pid: u32) -> Option<String> {
    let process = unsafe {
        // SAFETY: the process id came from the system snapshot and the handle is owned below.
        OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            0,
            pid,
        )
    };
    let process = OwnedHandle::new(process)?;
    process_environment_value(process.raw(), "CLAUDE_CONFIG_DIR")
}
