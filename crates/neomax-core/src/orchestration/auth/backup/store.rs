use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::io::{read_file, BoundedIoError, LocalFileSource};
use crate::{Engine, Error, Result};

use super::document::BackupDocument;
use super::legacy::write_legacy_fields;
use super::super::limits::backup_read_limits;
use super::names::safe_name;
use super::super::permissions::ensure_private_directory;
use super::super::types::{absolute_path, profile_name, FileSnapshot};
use super::super::writer::{CredentialWriter, FsCredentialWriter};

pub struct BackupStore {
    directory: PathBuf,
}

impl BackupStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn save(
        &self,
        engine: Engine,
        profile: &Path,
        snapshot: &FileSnapshot,
        timestamp: i64,
    ) -> Result<PathBuf> {
        self.save_with_purpose(engine, profile, snapshot, timestamp, "rotation")
    }

    pub fn save_safety(
        &self,
        engine: Engine,
        profile: &Path,
        snapshot: &FileSnapshot,
        timestamp: i64,
    ) -> Result<PathBuf> {
        self.save_with_purpose(engine, profile, snapshot, timestamp, "restore-safety")
    }

    fn save_with_purpose(
        &self,
        engine: Engine,
        profile: &Path,
        snapshot: &FileSnapshot,
        timestamp: i64,
        purpose: &str,
    ) -> Result<PathBuf> {
        ensure_private_directory(&self.directory)?;
        let document = BackupDocument::from_snapshot(engine, profile, snapshot, timestamp, purpose);
        let name = if document.purpose == "rotation" {
            // Preserve the profile suffix used by the legacy restore search.
            format!(
                "{}-{}-{}-{}.json",
                timestamp,
                engine.as_str(),
                Uuid::new_v4().simple(),
                safe_name(&profile_name(profile))
            )
        } else {
            format!(
                "{}-{}-{}-{}.json",
                timestamp,
                engine.as_str(),
                safe_name(&profile_name(profile)),
                Uuid::new_v4().simple()
            )
        };
        let path = self.directory.join(name);
        let mut value = serde_json::to_value(&document)?;
        if document.purpose == "rotation" {
            write_legacy_fields(&mut value, engine, snapshot, timestamp);
        }
        let mut bytes = serde_json::to_vec_pretty(&value)?;
        bytes.push(b'\n');
        FsCredentialWriter.write_atomic(&path, &bytes)?;
        Ok(path)
    }

    pub fn load(&self, path: &Path) -> Result<BackupDocument> {
        let bytes = read_file(&LocalFileSource, path, backup_read_limits()).map_err(|error| {
            match error {
                BoundedIoError::NotFound { path } => Error::NotFound(path.display().to_string()),
                other => other.into(),
            }
        })?;
        super::legacy::parse_document(path, &bytes)
    }

    pub fn load_for_profile(&self, path: &Path, profile: &Path) -> Result<BackupDocument> {
        let mut document = self.load(path)?;
        if !document.matches_profile(profile) {
            return Err(Error::InvalidArgument(format!(
                "backup {} belongs to another profile",
                path.display()
            )));
        }
        document.bind_profile(profile);
        Ok(document)
    }

    pub fn latest(
        &self,
        engine: Engine,
        profile: &Path,
    ) -> Result<Option<(PathBuf, BackupDocument)>> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let target = absolute_path(profile);
        let mut matches = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(mut document) = self.load(&path) else {
                continue;
            };
            if document.engine == engine
                && document.purpose == "rotation"
                && document.matches_profile(&target)
            {
                document.bind_profile(&target);
                matches.push((document.timestamp, path, document));
            }
        }
        matches.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        Ok(matches.pop().map(|(_, path, document)| (path, document)))
    }
}
