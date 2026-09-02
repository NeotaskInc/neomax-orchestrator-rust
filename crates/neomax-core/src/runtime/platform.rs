//! Platform identifiers and child-process environment allowlists.

/// The platform used for path and process-environment policy.
///
/// This is explicit so Windows behavior can be tested on other hosts without
/// mutating the process environment or pretending a Unix path is a Windows
/// path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePlatform {
    Unix,
    Windows,
    Other,
}

impl Default for RuntimePlatform {
    fn default() -> Self {
        Self::current()
    }
}

impl RuntimePlatform {
    pub const fn current() -> Self {
        #[cfg(unix)]
        {
            return Self::Unix;
        }
        #[cfg(windows)]
        {
            return Self::Windows;
        }
        #[allow(unreachable_code)]
        Self::Other
    }

    pub const fn is_windows(self) -> bool {
        matches!(self, Self::Windows)
    }
}

/// Environment names retained when a provider discovery process is started
/// with a cleared environment.
pub const WINDOWS_CHILD_ENVIRONMENT: &[&str] = &[
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "SystemRoot",
    "ComSpec",
    "PATH",
    "TEMP",
    "TMP",
];

pub(crate) const UNIX_CHILD_ENVIRONMENT: &[&str] = &["PATH", "LANG", "LC_ALL", "TERM"];
