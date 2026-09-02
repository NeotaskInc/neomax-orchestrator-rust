use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Engine, Error, Result};

use super::super::files::{path_exists, read_bounded};
use super::super::paths::InstallPaths;
use super::super::types::{KIMI_AGENT_RECORD, WORKFLOWS};
use super::support::profile_home;

pub(super) const WORKFLOW_SCHEMA_VERSION: u32 = 1;
pub(super) const MAX_SETTINGS_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkflowManifest {
    pub schema_version: u32,
    pub product: String,
    #[serde(default)]
    pub home: String,
    pub files: Vec<WorkflowFile>,
    pub hooks: Vec<HookRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkflowFile {
    pub path: String,
    pub engine: Engine,
    pub workflow: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HookRecord {
    pub path: String,
    #[serde(default)]
    pub event: String,
    pub command: String,
}

#[derive(Debug)]
pub(crate) struct WorkflowStage {
    pub replacements: Vec<super::super::transaction::Replacement>,
    pub _guards: Vec<crate::io::PathGuard>,
}

impl WorkflowManifest {
    pub(crate) fn empty() -> Self {
        Self {
            schema_version: WORKFLOW_SCHEMA_VERSION,
            product: "neomax".into(),
            home: String::new(),
            files: Vec::new(),
            hooks: Vec::new(),
        }
    }

    pub(crate) fn read(paths: &InstallPaths) -> Result<Option<Self>> {
        let path = paths.workflow_manifest_path();
        if !path_exists(&path) {
            return Ok(None);
        }
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err(Error::InvalidState {
                path,
                message: "workflow manifest must be a regular file".into(),
            });
        }
        crate::io::verify_private_path(&path)?;
        let value = serde_json::from_slice::<Self>(&read_bounded(&path, MAX_SETTINGS_BYTES)?)
            .map_err(|error| Error::InvalidState {
                path: path.clone(),
                message: error.to_string(),
            })?;
        value.validate(&path)?;
        Ok(Some(value))
    }

    fn validate(&self, path: &Path) -> Result<()> {
        if self.schema_version != WORKFLOW_SCHEMA_VERSION || self.product != "neomax" {
            return Err(Error::InvalidState {
                path: path.to_path_buf(),
                message: "unsupported workflow manifest".into(),
            });
        }
        let home = if self.home.is_empty() {
            profile_home()?
        } else {
            PathBuf::from(&self.home)
        };
        if !home.is_absolute() {
            return Err(Error::InvalidState {
                path: path.to_path_buf(),
                message: "workflow manifest home is not absolute".into(),
            });
        }
        for file in &self.files {
            let valid_workflow = WORKFLOWS.contains(&file.workflow.as_str())
                || (file.workflow == KIMI_AGENT_RECORD && file.engine == Engine::Kimi);
            if !valid_workflow
                || file.path.is_empty()
                || file.sha256.len() != 64
                || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                || !Path::new(&file.path).is_absolute()
                || !Path::new(&file.path).starts_with(&home)
            {
                return Err(Error::InvalidState {
                    path: path.to_path_buf(),
                    message: "workflow manifest contains an invalid file record".into(),
                });
            }
        }
        for hook in &self.hooks {
            if hook.path.is_empty()
                || hook.command.is_empty()
                || !Path::new(&hook.path).is_absolute()
                || !Path::new(&hook.path).starts_with(&home)
            {
                return Err(Error::InvalidState {
                    path: path.to_path_buf(),
                    message: "workflow manifest contains an invalid hook record".into(),
                });
            }
        }
        Ok(())
    }
}
