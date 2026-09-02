use std::path::{Path, PathBuf};

use crate::{Error, Result};

pub fn validate_plan_id(value: &str) -> Result<()> {
    crate::git::workspace::validate_plan_id(value)
}

pub fn plan_path(directory: &Path, plan_id: &str) -> Result<PathBuf> {
    validate_plan_id(plan_id)?;
    let path = directory.join(format!("{plan_id}.json"));
    ensure_child(directory, &path, plan_id)
}

pub fn lock_path(directory: &Path, plan_id: &str) -> Result<PathBuf> {
    validate_plan_id(plan_id)?;
    let path = directory.join(format!("{plan_id}.lock"));
    ensure_child(directory, &path, plan_id)
}

fn ensure_child(directory: &Path, path: &Path, plan_id: &str) -> Result<PathBuf> {
    if path.parent() != Some(directory) || path.file_name().is_none() {
        return Err(Error::InvalidArgument(format!(
            "scheduler plan id {plan_id:?} escapes the plans directory"
        )));
    }
    Ok(path.to_path_buf())
}
