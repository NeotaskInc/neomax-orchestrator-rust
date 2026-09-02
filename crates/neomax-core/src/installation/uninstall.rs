use std::fs;
use std::path::Path;

use crate::io::PathGuard;
use crate::{Error, Result};

use super::files::{path_exists, remove_empty_parent, sha256};
use super::manifest::installed_path;
use super::paths::InstallPaths;
use super::transaction::remove_all;
use super::types::{InstallManifest, ManifestKind, UninstallOptions, UninstallReport};

pub fn uninstall(options: UninstallOptions) -> Result<UninstallReport> {
    let paths = options.paths.unwrap_or(InstallPaths::discover()?);
    paths.validate_destinations()?;
    let report = {
        let _root_guard = PathGuard::for_directory(&paths.root)?;
        let _bin_guard = PathGuard::for_directory(&paths.bin_dir)?;
        let _share_guard = PathGuard::for_directory(&paths.share_dir)?;
        if !paths.manifest_path().is_file() {
            return Err(Error::NotFound(format!(
                "no Neomax installation manifest exists at {}",
                paths.manifest_path().display()
            )));
        }
        crate::atomic::with_exclusive_lock(&paths.lock_path, || {
        let manifest = InstallManifest::read(&paths)?.ok_or_else(|| {
            Error::NotFound(format!(
                "no Neomax installation manifest exists at {}",
                paths.manifest_path().display()
            ))
        })?;
        let workflow_manifest = super::workflows::WorkflowManifest::read(&paths)?;
        super::workflows::preflight_uninstall(&paths, workflow_manifest.as_ref(), options.force)?;
        let mut targets = Vec::new();
        #[cfg(windows)]
        let mut deferred = Vec::new();
        let mut removed = Vec::new();
        let mut preserved = Vec::new();
        for file in &manifest.files {
            let target = installed_path(&paths, &file.path);
            if !path_exists(&target) {
                preserved.push(file.path.clone());
                continue;
            }
            verify_owned(file, &target, options.force, &paths)?;
            #[cfg(windows)]
            if super::windows::is_current_executable(&target) {
                deferred.push(target);
                removed.push(file.path.clone());
                continue;
            }
            targets.push(target);
            removed.push(file.path.clone());
        }
        let manifest_path = paths.manifest_path();
        if path_exists(&manifest_path) {
            targets.push(manifest_path);
            removed.push("share/neomax/install-manifest.json".into());
        }
        #[cfg(windows)]
        for target in &deferred {
            super::windows::defer_delete(target)?;
        }
        if !targets.is_empty() {
            remove_all(&targets, paths.root.parent().unwrap_or(Path::new(".")))?;
        }
        super::workflows::remove_owned_files(&paths, workflow_manifest.as_ref(), options.force)?;
        if workflow_manifest.is_some() {
            super::workflows::remove_manifest(&paths)?;
            removed.push("share/neomax/workflow-install-manifest.json".into());
        }
        Ok(UninstallReport {
            product: "neomax".into(),
            bin_dir: paths.bin_dir.clone(),
            share_dir: paths.share_dir.clone(),
            removed,
            preserved,
        })
        })?
    };
    remove_empty_parent(&paths.share_dir);
    Ok(report)
}

fn verify_owned(
    file: &super::types::ManifestFile,
    target: &Path,
    force: bool,
    paths: &InstallPaths,
) -> Result<()> {
    match &file.kind {
        ManifestKind::File => {
            if !fs::symlink_metadata(target)?.file_type().is_file() {
                return Err(Error::Conflict(format!(
                    "refusing to remove a non-file installation path {}; pass --force only for a regular file",
                    target.display()
                )));
            }
            if force {
                return Ok(());
            }
            let expected = file.sha256.as_deref().ok_or_else(|| Error::InvalidState {
                path: paths.manifest_path(),
                message: format!("file {} has no recorded hash", file.path),
            })?;
            if sha256(target)? != expected {
                return Err(Error::Conflict(format!(
                    "refusing to remove modified installed file {}; pass --force to remove it",
                    target.display()
                )));
            }
        }
        ManifestKind::Symlink => {
            if !fs::symlink_metadata(target)?.file_type().is_symlink() {
                return Err(Error::Conflict(format!(
                    "refusing to remove a non-link installation path {}; pass --force only for the Neomax link",
                    target.display()
                )));
            }
            if force {
                return Ok(());
            }
            let expected = file.target.as_deref().unwrap_or_default();
            if fs::read_link(target).ok().as_deref() != Some(Path::new(expected)) {
                return Err(Error::Conflict(format!(
                    "refusing to remove modified installed alias {}; pass --force to remove it",
                    target.display()
                )));
            }
        }
    }
    Ok(())
}
