#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Live,
    Dead,
    Unknown,
}

pub trait SessionLiveness: Send + Sync {
    fn state(&self, session: &str) -> SessionState;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnknownSessions;

impl SessionLiveness for UnknownSessions {
    fn state(&self, _session: &str) -> SessionState {
        SessionState::Unknown
    }
}
