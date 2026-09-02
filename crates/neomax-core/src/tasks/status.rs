use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TaskStatus {
    #[default]
    Todo,
    Doing,
    Blocked,
    Done,
    Merged,
    Dropped,
    Unknown(String),
}

impl TaskStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Merged => "merged",
            Self::Dropped => "dropped",
            Self::Unknown(value) => value,
        }
    }

    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }

    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done | Self::Merged | Self::Dropped)
    }
}

impl From<&str> for TaskStatus {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "todo" => Self::Todo,
            "doing" => Self::Doing,
            "blocked" => Self::Blocked,
            "done" => Self::Done,
            "merged" => Self::Merged,
            "dropped" => Self::Dropped,
            _ => Self::Unknown(value.into()),
        }
    }
}

impl Serialize for TaskStatus {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from(value.as_str()))
    }
}
