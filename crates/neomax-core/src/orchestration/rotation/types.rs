use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationWindow {
    FiveHour,
    Weekly,
}

impl RotationWindow {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FiveHour => "5h",
            Self::Weekly => "weekly",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotationAdvice {
    pub rotate: bool,
    pub reason: String,
    #[serde(default)]
    pub window: Option<RotationWindow>,
}
