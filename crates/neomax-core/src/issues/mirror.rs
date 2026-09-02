use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MirrorState {
    #[default]
    Local,
    Open,
    Closed,
    Unknown(String),
}

impl MirrorState {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Local => "local",
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Unknown(value) => value,
        }
    }
}

impl From<&str> for MirrorState {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "local" => Self::Local,
            "open" => Self::Open,
            "closed" => Self::Closed,
            _ => Self::Unknown(value.into()),
        }
    }
}

impl Serialize for MirrorState {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MirrorState {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from(value.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueMirror {
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    pub number: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default = "default_mirror_state")]
    pub state: MirrorState,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl IssueMirror {
    pub fn local() -> Self {
        Self {
            number: None,
            url: None,
            state: MirrorState::Local,
            extra: BTreeMap::new(),
        }
    }
}

impl Default for IssueMirror {
    fn default() -> Self {
        Self::local()
    }
}

fn default_mirror_state() -> MirrorState {
    MirrorState::Local
}

fn deserialize_optional_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        serde_json::Value::String(value) => Some(value),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }))
}
