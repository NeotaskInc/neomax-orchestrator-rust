use chrono::{DateTime, Utc};

pub trait RunClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl RunClock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
