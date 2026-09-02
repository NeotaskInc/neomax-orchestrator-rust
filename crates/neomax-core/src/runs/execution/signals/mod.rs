#[cfg(not(any(unix, windows)))]
mod other;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(not(any(unix, windows)))]
use self::other as platform;
#[cfg(unix)]
use self::unix as platform;
#[cfg(windows)]
use self::windows as platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SignalReason {
    Interrupt,
    Terminate,
    Hangup,
    Other(u32),
}

impl SignalReason {
    pub(super) const fn number(self) -> i32 {
        match self {
            Self::Interrupt => signal_number::INTERRUPT,
            Self::Terminate => signal_number::TERMINATE,
            Self::Hangup => signal_number::HANGUP,
            Self::Other(value) => value as i32,
        }
    }

    #[cfg(unix)]
    pub(super) const fn from_unix(value: i32) -> Self {
        match value {
            signal_number::INTERRUPT => Self::Interrupt,
            signal_number::TERMINATE => Self::Terminate,
            signal_number::HANGUP => Self::Hangup,
            other => Self::Other(other as u32),
        }
    }

    #[cfg(windows)]
    pub(super) const fn from_console(value: u32) -> Self {
        match value {
            0 => Self::Interrupt,
            1 => Self::Terminate,
            2 => Self::Hangup,
            5 | 6 => Self::Terminate,
            other => Self::Other(other),
        }
    }
}

#[cfg(unix)]
mod signal_number {
    pub(super) const INTERRUPT: i32 = libc::SIGINT;
    pub(super) const TERMINATE: i32 = libc::SIGTERM;
    pub(super) const HANGUP: i32 = libc::SIGHUP;
}

#[cfg(windows)]
mod signal_number {
    pub(super) const INTERRUPT: i32 = 0;
    pub(super) const TERMINATE: i32 = 1;
    pub(super) const HANGUP: i32 = 2;
}

#[cfg(not(any(unix, windows)))]
mod signal_number {
    pub(super) const INTERRUPT: i32 = 2;
    pub(super) const TERMINATE: i32 = 15;
    pub(super) const HANGUP: i32 = 1;
}

pub(super) use platform::SignalGuard;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newly_installed_subscription_starts_quiet() {
        let mut guard = SignalGuard::install().unwrap();
        assert_eq!(guard.poll(), None);
        assert_eq!(guard.last_signal(), None);
    }

    #[cfg(unix)]
    #[test]
    fn maps_all_unix_control_signals() {
        assert_eq!(
            SignalReason::from_unix(libc::SIGINT),
            SignalReason::Interrupt
        );
        assert_eq!(
            SignalReason::from_unix(libc::SIGTERM),
            SignalReason::Terminate
        );
        assert_eq!(SignalReason::from_unix(libc::SIGHUP), SignalReason::Hangup);
        assert_eq!(SignalReason::Terminate.number(), libc::SIGTERM);
    }
}
