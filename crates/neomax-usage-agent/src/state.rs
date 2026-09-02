use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use neomax_core::atomic::write_json_atomic;
use neomax_core::io::PathGuard;

use crate::io::{MAX_STATE_BYTES, read_string};

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaintenanceState {
    pub last_rotation_attempt: Option<i64>,
    pub last_keepalive_attempt: Option<i64>,
    pub last_worktree_tidy_attempt: Option<i64>,
    pub last_rotation: Option<MaintenanceSummary>,
    pub last_keepalive: Option<MaintenanceSummary>,
    pub last_worktree_tidy: Option<MaintenanceSummary>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MaintenanceSummary {
    pub attempted_at: i64,
    pub completed_at: Option<i64>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchState {
    pub files: BTreeMap<String, u64>,
    pub codex_total: BTreeMap<String, u64>,
    pub codex_model: BTreeMap<String, String>,
    pub database_rows: BTreeMap<String, String>,
    pub baselined: bool,
    pub maintenance: MaintenanceState,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl WatchState {
    pub fn load(path: &Path) -> Result<Self> {
        let _path_guard = PathGuard::for_path(path)
            .with_context(|| format!("validate watch state path {}", path.display()))?;
        let contents = match read_string(path, MAX_STATE_BYTES) {
            Ok(contents) => contents,
            Err(_) if !path.exists() => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("read watch state {}", path.display()));
            }
        };
        let state = serde_json::from_str(&contents)
            .with_context(|| format!("decode watch state {}", path.display()))?;
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_json_atomic(path, self)
            .with_context(|| format!("write watch state {}", path.display()))
    }

    pub fn reset(&mut self) {
        self.files.clear();
        self.codex_total.clear();
        self.codex_model.clear();
        self.database_rows.clear();
        self.baselined = false;
        self.maintenance = MaintenanceState::default();
    }

    pub fn has_ledger(path: &Path) -> Result<bool> {
        let _directory_guard = PathGuard::for_directory(path)
            .with_context(|| format!("validate usage ledger directory {}", path.display()))?;
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        Ok(entries.flatten().any(|entry| {
            entry.path().extension().and_then(|value| value.to_str()) == Some("jsonl")
        }))
    }

    pub fn clear_ledger(path: &Path) -> Result<usize> {
        let _directory_guard = PathGuard::for_directory(path)
            .with_context(|| format!("validate usage ledger directory {}", path.display()))?;
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.into()),
        };
        let mut removed = 0;
        for entry in entries {
            let entry = entry?;
            let target = entry.path();
            if target.extension().and_then(|value| value.to_str()) != Some("jsonl") {
                continue;
            }
            let _target_guard = PathGuard::for_path(&target)
                .with_context(|| format!("validate usage ledger path {}", target.display()))?;
            let metadata = fs::symlink_metadata(&target)?;
            if is_link_like(&metadata) {
                bail!(
                    "refusing to remove a symlink or reparse usage ledger path: {}",
                    target.display()
                );
            }
            fs::remove_file(target)?;
            removed += 1;
        }
        Ok(removed)
    }

    pub fn compact(&mut self) {
        self.files.retain(|path, _| !path.is_empty());
        self.codex_total.retain(|id, _| !id.is_empty());
        self.codex_model.retain(|id, _| !id.is_empty());
        self.database_rows.retain(|id, _| !id.is_empty());
    }

    pub fn validate(&self) -> Result<()> {
        if self.files.keys().any(|path| path.is_empty()) {
            bail!("watch state contains an empty source path");
        }
        Ok(())
    }
}

fn is_link_like(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
