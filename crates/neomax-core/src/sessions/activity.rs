use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityState {
    Active,
    Idle,
    Stopped,
    #[default]
    Unknown,
}

impl ActivityState {
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Idle | Self::Stopped)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActivityInput {
    pub now: i64,
    pub last_modified: i64,
    pub active_window: i64,
    pub terminal: bool,
    pub progress: bool,
    pub live: Option<bool>,
    pub archived: bool,
}

pub fn classify_activity(input: ActivityInput) -> ActivityState {
    if input.archived {
        return ActivityState::Stopped;
    }
    if input.live == Some(false) {
        return ActivityState::Stopped;
    }
    if input.terminal {
        return ActivityState::Idle;
    }
    if input.live == Some(true) {
        return ActivityState::Active;
    }
    let fresh = input.now.saturating_sub(input.last_modified) <= input.active_window.max(0);
    if fresh && input.progress {
        return ActivityState::Active;
    }
    if fresh {
        ActivityState::Idle
    } else {
        ActivityState::Unknown
    }
}

pub fn age_seconds(now: i64, last_active: Option<i64>) -> Option<i64> {
    last_active.map(|value| now.saturating_sub(value).max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_metadata_wins_over_recent_file_mtime() {
        assert_eq!(
            classify_activity(ActivityInput {
                now: 100,
                last_modified: 99,
                active_window: 60,
                terminal: true,
                progress: true,
                ..ActivityInput::default()
            }),
            ActivityState::Idle
        );
    }

    #[test]
    fn live_codex_session_stays_active_between_turns() {
        assert_eq!(
            classify_activity(ActivityInput {
                now: 100,
                last_modified: 1,
                active_window: 30,
                live: Some(true),
                ..ActivityInput::default()
            }),
            ActivityState::Active
        );
    }
}
