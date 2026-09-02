use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::{Error, Result};

use super::{SignalReason, signal_number};

const SIGNALS: [i32; 3] = [
    signal_number::INTERRUPT,
    signal_number::TERMINATE,
    signal_number::HANGUP,
];

static USERS: AtomicUsize = AtomicUsize::new(0);
static GENERATION: AtomicUsize = AtomicUsize::new(0);
static LAST_SIGNAL: AtomicI32 = AtomicI32::new(0);
static PREVIOUS: OnceLock<Mutex<Vec<(i32, libc::sigaction)>>> = OnceLock::new();

fn previous() -> &'static Mutex<Vec<(i32, libc::sigaction)>> {
    PREVIOUS.get_or_init(|| Mutex::new(Vec::new()))
}

extern "C" fn handler(signal: i32) {
    LAST_SIGNAL.store(signal, Ordering::Relaxed);
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

pub struct SignalGuard {
    generation: usize,
    last: Option<SignalReason>,
}

impl SignalGuard {
    pub fn install() -> Result<Self> {
        if USERS.fetch_add(1, Ordering::AcqRel) == 0 {
            if let Err(error) = install_handlers() {
                USERS.fetch_sub(1, Ordering::AcqRel);
                return Err(error);
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
        let signal = SignalReason::from_unix(LAST_SIGNAL.load(Ordering::Relaxed));
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
            restore_handlers();
        }
    }
}

fn install_handlers() -> Result<()> {
    let mut installed = Vec::new();
    for signal in SIGNALS {
        // SAFETY: sigaction is a C ABI value whose zero state is valid; the
        // mask, handler, and flags are initialized before libc reads it.
        let mut new_action = unsafe { std::mem::zeroed::<libc::sigaction>() };
        let mut old_action = MaybeUninit::<libc::sigaction>::uninit();
        // SAFETY: sigemptyset and sigaction receive valid pointers to local sigaction values.
        unsafe {
            if libc::sigemptyset(&mut new_action.sa_mask) != 0 {
                restore_installed(&installed);
                return Err(std::io::Error::last_os_error().into());
            }
            new_action.sa_sigaction = handler as *const () as usize;
            new_action.sa_flags = 0;
            if libc::sigaction(signal, std::ptr::null(), old_action.as_mut_ptr()) != 0
                || libc::sigaction(signal, &new_action, std::ptr::null_mut()) != 0
            {
                restore_installed(&installed);
                return Err(std::io::Error::last_os_error().into());
            }
        }
        // SAFETY: sigaction initialized old_action on successful query above.
        installed.push((signal, unsafe { old_action.assume_init() }));
    }
    match previous().lock() {
        Ok(mut slot) => {
            *slot = installed;
            Ok(())
        }
        Err(_) => {
            restore_installed(&installed);
            Err(Error::Message("signal handler state lock poisoned".into()))
        }
    }
}

fn restore_installed(installed: &[(i32, libc::sigaction)]) {
    for (signal, action) in installed.iter().rev() {
        // SAFETY: action came from a successful sigaction query and the signal is one we installed.
        unsafe {
            let _ = libc::sigaction(*signal, action, std::ptr::null_mut());
        }
    }
}

fn restore_handlers() {
    if let Ok(mut slot) = previous().lock() {
        restore_installed(&slot);
        slot.clear();
    }
}
