use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::super::files::{path_exists, remove_empty_parent, sha256};
use super::super::paths::InstallPaths;
use super::hooks::{read_settings, remove_hooks, unique_settings, write_json_value_atomic};
use super::manifest::WorkflowManifest;

pub(crate) fn preflight_uninstall(
    _paths: &InstallPaths,
    manifest: Option<&WorkflowManifest>,
    force: bool,
) -> Result<()> {
    let Some(manifest) = manifest else {
        return Ok(());
    };
    let mut guards = Vec::new();
    for file in &manifest.files {
        let target = PathBuf::from(&file.path);
        guards.push(crate::io::PathGuard::for_path(&target)?);
        if !path_exists(&target) {
            continue;
        }
        if !fs::symlink_metadata(&target)?.file_type().is_file() {
            return Err(Error::Conflict(format!(
                "refusing to remove a non-file workflow path {}; pass --force only for a regular file",
                target.display()
            )));
        }
        if !force && sha256(&target)? != file.sha256 {
            return Err(Error::Conflict(format!(
                "refusing to remove modified workflow {}; pass --force to remove it",
                target.display()
            )));
        }
    }
    for hook in unique_settings(manifest) {
        let target = PathBuf::from(hook);
        guards.push(crate::io::PathGuard::for_path(&target)?);
        if path_exists(&target) && !fs::symlink_metadata(&target)?.file_type().is_file() {
            return Err(Error::Conflict(format!(
                "refusing to update a non-file Claude settings path {}",
                target.display()
            )));
        }
        if path_exists(&target) {
            let _ = read_settings(&target)?;
        }
    }
    Ok(())
}

pub(crate) fn remove_owned_files(
    _paths: &InstallPaths,
    manifest: Option<&WorkflowManifest>,
    force: bool,
) -> Result<()> {
    let Some(manifest) = manifest else {
        return Ok(());
    };
    let mut guards = Vec::new();
    for file in &manifest.files {
        guards.push(crate::io::PathGuard::for_path(Path::new(&file.path))?);
    }
    for settings_path in unique_settings(manifest) {
        guards.push(crate::io::PathGuard::for_path(Path::new(&settings_path))?);
    }
    preflight_uninstall(_paths, Some(manifest), force)?;
    for file in &manifest.files {
        let target = PathBuf::from(&file.path);
        if path_exists(&target) {
            fs::remove_file(&target)?;
        }
        remove_empty_parent(&target);
    }
    for settings_path in unique_settings(manifest) {
        let target = PathBuf::from(&settings_path);
        if !path_exists(&target) {
            continue;
        }
        let Some(mut settings) = read_settings(&target)? else {
            continue;
        };
        let owned = manifest
            .hooks
            .iter()
            .filter(|hook| hook.path == settings_path)
            .map(|hook| (hook.event.clone(), hook.command.clone()))
            .collect::<BTreeSet<_>>();
        remove_hooks(&mut settings, &owned);
        write_json_value_atomic(&target, &settings)?;
    }
    Ok(())
}

pub(crate) fn remove_manifest(paths: &InstallPaths) -> Result<()> {
    let path = paths.workflow_manifest_path();
    if path_exists(&path) {
        let _guard = crate::io::PathGuard::for_path(&path)?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            return Err(Error::Conflict(format!(
                "refusing to remove a non-file workflow manifest {}",
                path.display()
            )));
        }
        fs::remove_file(path)?;
    }
    Ok(())
}
