use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use crate::Result;

use super::SignalReason;

const CTRL_C_EVENT: u32 = 0;
const CTRL_BREAK_EVENT: u32 = 1;
const CTRL_CLOSE_EVENT: u32 = 2;
const CTRL_LOGOFF_EVENT: u32 = 5;
const CTRL_SHUTDOWN_EVENT: u32 = 6;

static USERS: AtomicUsize = AtomicUsize::new(0);
static GENERATION: AtomicUsize = AtomicUsize::new(0);
static LAST_CONTROL: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn handler(control: u32) -> i32 {
    match control {
        CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT
        | CTRL_SHUTDOWN_EVENT => {
            LAST_CONTROL.store(control, Ordering::Relaxed);
            GENERATION.fetch_add(1, Ordering::Relaxed);
            1
        }
        _ => 0,
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}

pub struct SignalGuard {
    generation: usize,
    last: Option<SignalReason>,
}

impl SignalGuard {
    pub fn install() -> Result<Self> {
        if USERS.fetch_add(1, Ordering::AcqRel) == 0 {
            // SAFETY: handler has the documented console-control callback ABI.
            if unsafe { SetConsoleCtrlHandler(Some(handler), 1) } == 0 {
                USERS.fetch_sub(1, Ordering::AcqRel);
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(Self {
            generation: GENERATION.load(Ordering::Acquire),
            last: None,
        })
    }

    pub fn poll(&mut self) -> Option<SignalReason> {
        let current = GENERATION.load(Ordering::Acquire);
        if current == self.generation {
            return None;
        }
        self.generation = current;
        let signal = SignalReason::from_console(LAST_CONTROL.load(Ordering::Relaxed));
        self.last = Some(signal);
        Some(signal)
    }

    pub fn last_signal(&mut self) -> Option<SignalReason> {
        self.poll().or(self.last)
    }
}

impl Drop for SignalGuard {
    fn drop(&mut self) {
        if USERS.fetch_sub(1, Ordering::AcqRel) == 1 {
            // SAFETY: removes the exact callback installed by this module.
            unsafe {
                let _ = SetConsoleCtrlHandler(Some(handler), 0);
            }
        }
    }
}
