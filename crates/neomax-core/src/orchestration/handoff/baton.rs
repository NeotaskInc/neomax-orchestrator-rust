use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::atomic::{read_json, with_exclusive_lock, write_json_atomic};
use crate::{Engine, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountId(String);

impl AccountId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for AccountId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for AccountId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl Serialize for AccountId {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self.0.parse::<u64>() {
            Ok(value) => serializer.serialize_u64(value),
            Err(_) => serializer.serialize_str(&self.0),
        }
    }
}

impl<'de> Deserialize<'de> for AccountId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct AccountIdVisitor;

        impl<'de> Visitor<'de> for AccountIdVisitor {
            type Value = AccountId;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an account number or account label")
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Self::Value, E> {
                Ok(AccountId::new(value.to_string()))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Self::Value, E> {
                if value < 0 {
                    return Err(E::custom("account number cannot be negative"));
                }
                Ok(AccountId::new(value.to_string()))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
                Ok(AccountId::new(value))
            }

            fn visit_string<E: de::Error>(
                self,
                value: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(AccountId::new(value))
            }
        }

        deserializer.deserialize_any(AccountIdVisitor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoffBaton {
    pub ts: i64,
    pub engine: Engine,
    pub from_account: AccountId,
    pub to_account: Option<AccountId>,
    pub reason: String,
    pub cwd: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

pub struct HandoffStore {
    path: PathBuf,
    lock: PathBuf,
}

impl HandoffStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let lock = path.with_extension("lock");
        Self { path, lock }
    }

    pub fn at_state_dir(state: impl AsRef<Path>) -> Self {
        Self::new(state.as_ref().join("handoff.json"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock
    }

    pub fn load(&self) -> Result<Option<HandoffBaton>> {
        match read_json(&self.path) {
            Ok(baton) => Ok(Some(baton)),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, baton: &HandoffBaton) -> Result<()> {
        with_exclusive_lock(&self.lock, || write_json_atomic(&self.path, baton))
    }

    pub fn clear(&self) -> Result<bool> {
        with_exclusive_lock(&self.lock, || {
            let _path_guard = crate::io::PathGuard::for_path(&self.path)?;
            crate::io::reject_reparse_components(&self.path)?;
            match fs::remove_file(&self.path) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error.into()),
            }
        })
    }
}
