use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfHealPolicy {
    pub max_age: Duration,
    pub max_attempts: u32,
    pub max_batch: usize,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub max_history: usize,
}

impl Default for SelfHealPolicy {
    fn default() -> Self {
        Self {
            max_age: Duration::from_secs(6 * 60 * 60),
            max_attempts: 5,
            max_batch: 5,
            initial_backoff: Duration::from_secs(30),
            max_backoff: Duration::from_secs(30 * 60),
            max_history: 32,
        }
    }
}

impl SelfHealPolicy {
    pub fn backoff_seconds(&self, attempt: u32) -> i64 {
        let shift = attempt.saturating_sub(1).min(20);
        let multiplier = 1_u64 << shift;
        let seconds = self
            .initial_backoff
            .as_secs()
            .saturating_mul(multiplier)
            .min(self.max_backoff.as_secs());
        i64::try_from(seconds).unwrap_or(i64::MAX)
    }
}
