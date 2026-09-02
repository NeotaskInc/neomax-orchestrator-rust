use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum IssueStatus {
    #[default]
    Open,
    Claimed,
    Fixing,
    Blocked,
    Done,
    Closed,
    Unknown(String),
}

impl IssueStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Open => "open",
            Self::Claimed => "claimed",
            Self::Fixing => "fixing",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Closed => "closed",
            Self::Unknown(value) => value,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Closed)
    }

    pub fn can_transition_to(&self, next: &Self) -> bool {
        if self == next {
            return true;
        }
        if matches!(self, Self::Unknown(_)) || matches!(next, Self::Unknown(_)) {
            return true;
        }
        match self {
            Self::Open | Self::Claimed | Self::Fixing | Self::Blocked => true,
            Self::Done | Self::Closed => matches!(next, Self::Open | Self::Done | Self::Closed),
            Self::Unknown(_) => true,
        }
    }

    pub fn transition_error(&self, next: &Self, key: &str) -> Error {
        Error::Conflict(format!(
            "issue {key} cannot transition from {} to {}",
            self.as_str(),
            next.as_str()
        ))
    }
}

impl From<&str> for IssueStatus {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "open" => Self::Open,
            "claimed" => Self::Claimed,
            "fixing" => Self::Fixing,
            "blocked" => Self::Blocked,
            "done" => Self::Done,
            "closed" => Self::Closed,
            _ => Self::Unknown(value.into()),
        }
    }
}

impl Serialize for IssueStatus {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for IssueStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from(value.as_str()))
    }
}
