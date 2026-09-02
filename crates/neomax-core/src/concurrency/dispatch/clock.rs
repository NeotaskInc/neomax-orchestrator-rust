use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub trait AdmissionClock: Send + Sync {
    fn now(&self) -> f64;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemAdmissionClock;

impl AdmissionClock for SystemAdmissionClock {
    fn now(&self) -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs_f64())
            .unwrap_or_default()
    }
}

pub trait OwnerLiveness: Send + Sync {
    fn is_live(&self, pid: u32) -> bool;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemOwnerLiveness;

impl OwnerLiveness for SystemOwnerLiveness {
    fn is_live(&self, pid: u32) -> bool {
        if pid == std::process::id() {
            return true;
        }
        #[cfg(unix)]
        {
            let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
            if result == 0 {
                true
            } else {
                std::io::Error::last_os_error()
                    .raw_os_error()
                    .is_some_and(|error| error == libc::EPERM)
            }
        }
        #[cfg(windows)]
        {
            windows_process_is_live(pid)
        }
        #[cfg(all(not(unix), not(windows)))]
        {
            let _ = pid;
            false
        }
    }
}

#[cfg(windows)]
fn windows_process_is_live(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, WAIT_FAILED,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    let process = unsafe {
        // SAFETY: The process identifier is copied from durable state, and the API does not retain
        // the pointer or mutate caller-owned memory.
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if process.is_null() {
        return match unsafe { GetLastError() } {
            ERROR_INVALID_PARAMETER => false,
            ERROR_ACCESS_DENIED => true,
            _ => true,
        };
    }

    let state = unsafe {
        // SAFETY: `process` is a valid handle returned by OpenProcess and is closed below.
        WaitForSingleObject(process, 0)
    };
    unsafe {
        // SAFETY: `process` is the owned handle returned by OpenProcess and is closed exactly once.
        CloseHandle(process);
    }

    match state {
        WAIT_OBJECT_0 => false,
        WAIT_TIMEOUT => true,
        WAIT_FAILED => true,
        _ => true,
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::{OwnerLiveness, SystemOwnerLiveness};

    #[test]
    fn windows_probe_keeps_the_current_process_live() {
        assert!(SystemOwnerLiveness.is_live(std::process::id()));
    }

    #[test]
    fn windows_probe_rejects_an_invalid_process_id() {
        assert!(!SystemOwnerLiveness.is_live(u32::MAX));
    }
}

pub(super) type SharedAdmissionClock = Arc<dyn AdmissionClock>;
pub(super) type SharedOwnerLiveness = Arc<dyn OwnerLiveness>;
