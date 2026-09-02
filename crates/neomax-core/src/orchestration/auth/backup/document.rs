use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Engine, Error, Result};

use super::encoding::{decode, encode};
use super::super::types::{absolute_path, profile_name, FileSnapshot};

pub(super) const BACKUP_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupDocument {
    pub version: u8,
    pub timestamp: i64,
    pub engine: Engine,
    pub profile: PathBuf,
    #[serde(default = "default_purpose")]
    pub purpose: String,
    #[serde(default)]
    pub credential_b64: Option<String>,
    #[serde(default)]
    pub identity_b64: Option<String>,
    #[serde(default)]
    pub auth_b64: Option<String>,
    #[serde(skip)]
    legacy_profile_name: Option<String>,
    #[serde(skip)]
    legacy_schema: bool,
    #[serde(skip)]
    legacy_empty_blob: bool,
}

impl BackupDocument {
    pub fn from_snapshot(
        engine: Engine,
        profile: &Path,
        snapshot: &FileSnapshot,
        timestamp: i64,
        purpose: impl Into<String>,
    ) -> Self {
        Self {
            version: BACKUP_VERSION,
            timestamp,
            engine,
            profile: absolute_path(profile),
            purpose: purpose.into(),
            credential_b64: encode(snapshot.credential.as_deref()),
            identity_b64: encode(snapshot.identity.as_deref()),
            auth_b64: encode(snapshot.auth.as_deref()),
            legacy_profile_name: None,
            legacy_schema: false,
            legacy_empty_blob: false,
        }
    }

    pub fn snapshot(&self) -> Result<FileSnapshot> {
        if self.version != BACKUP_VERSION {
            return Err(Error::InvalidState {
                path: self.profile.clone(),
                message: format!("unsupported credential backup version {}", self.version),
            });
        }
        if self.legacy_schema && self.legacy_empty_blob {
            return Err(Error::InvalidState {
                path: self.profile.clone(),
                message: "legacy credential backup has no credential blob".into(),
            });
        }
        Ok(FileSnapshot {
            credential: decode(self.credential_b64.as_deref())?,
            identity: decode(self.identity_b64.as_deref())?,
            auth: decode(self.auth_b64.as_deref())?,
        })
    }

    pub fn is_legacy(&self) -> bool {
        self.legacy_schema
    }

    pub fn matches_profile(&self, profile: &Path) -> bool {
        match self.legacy_profile_name.as_deref() {
            Some(name) => name == profile_name(profile),
            None => absolute_path(&self.profile) == absolute_path(profile),
        }
    }

    pub(super) fn bind_profile(&mut self, profile: &Path) {
        if self.legacy_schema {
            self.profile = absolute_path(profile);
        }
    }

    pub(super) fn from_legacy(
        engine: Engine,
        profile_name: String,
        timestamp: i64,
        credential: Option<Vec<u8>>,
        identity: Option<Vec<u8>>,
        auth: Option<Vec<u8>>,
        empty_blob: bool,
    ) -> Self {
        Self {
            version: BACKUP_VERSION,
            timestamp,
            engine,
            profile: PathBuf::from(&profile_name),
            purpose: "rotation".into(),
            credential_b64: encode(credential.as_deref()),
            identity_b64: encode(identity.as_deref()),
            auth_b64: encode(auth.as_deref()),
            legacy_profile_name: Some(profile_name),
            legacy_schema: true,
            legacy_empty_blob: empty_blob,
        }
    }
}

fn default_purpose() -> String {
    "rotation".into()
}
