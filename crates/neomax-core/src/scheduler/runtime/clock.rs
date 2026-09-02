use chrono::Utc;

pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        Utc::now().timestamp()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedClock {
    timestamp: i64,
}

impl FixedClock {
    pub const fn new(timestamp: i64) -> Self {
        Self { timestamp }
    }

    pub const fn timestamp(self) -> i64 {
        self.timestamp
    }
}

impl Clock for FixedClock {
    fn now(&self) -> i64 {
        self.timestamp
    }
}
