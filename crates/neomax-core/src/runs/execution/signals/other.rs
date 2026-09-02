use crate::Result;

use super::SignalReason;

pub struct SignalGuard;

impl SignalGuard {
    pub fn install() -> Result<Self> {
        Ok(Self)
    }

    pub fn poll(&mut self) -> Option<SignalReason> {
        None
    }

    pub fn last_signal(&mut self) -> Option<SignalReason> {
        None
    }
}
