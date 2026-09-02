use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::io::PathGuard;
use crate::{Error, Result};

use super::files::{copy_executable, copy_file, create_alias, path_exists, sha256};
use super::manifest::installed_path;
use super::package::Package;
use super::paths::{InstallPaths, PackageRoot};
use super::transaction::{replace_all, Replacement};
use super::types::{
    InstallManifest, InstallOptions, InstallReport, ManifestKind, ALIASES, ASSETS, AUXILIARIES,
    DOCS, KIMI_AGENT_ASSET, SHELL_ASSETS,
};

pub fn install(options: InstallOptions) -> Result<InstallReport> {
    let paths = options.paths.unwrap_or(InstallPaths::discover()?);
    let package_root = options
        .package_root
        .map(PackageRoot::new)
        .transpose()?
        .unwrap_or(PackageRoot::discover()?);
    let _package_guard = PathGuard::for_directory(package_root.path())?;
    let package = Package::load(&package_root)?;
    paths.validate_destinations()?;
    reject_overlapping_roots(&package.root, &paths)?;
    let _root_guard = PathGuard::ensure_directory(&paths.root)?;
    let _bin_guard = PathGuard::ensure_directory(&paths.bin_dir)?;
    let _share_guard = PathGuard::ensure_directory(&paths.share_dir)?;
    crate::atomic::with_exclusive_lock(&paths.lock_path, || {
        let previous = InstallManifest::read(&paths)?;
        let previous_workflows = super::workflows::WorkflowManifest::read(&paths)?;
        let stage = stage_package(&package, &paths)?;
        let manifest = InstallManifest::new(package.version.clone(), &stage.paths)?;
        manifest.write(&stage.manifest_path)?;
        let workflow_stage = super::workflows::stage(
            &package,
            &stage,
            &paths,
            previous_workflows.as_ref(),
            options.force,
            options.profile_home.as_deref(),
        )?;
        let entries = replacements(&manifest, &stage, &paths)
            .into_iter()
            .chain(workflow_stage.replacements.iter().cloned())
            .collect::<Vec<_>>();
        preflight_existing(&manifest, previous.as_ref(), &paths, options.force)?;
        replace_all(&entries, paths.root.parent().unwrap_or(Path::new(".")))?;
        Ok(InstallReport {
            product: "neomax".into(),
            version: package.version,
            bin_dir: paths.bin_dir.clone(),
            share_dir: paths.share_dir.clone(),
            aliases: ALIASES.iter().map(|name| (*name).into()).collect(),
            auxiliaries: AUXILIARIES.iter().map(|name| (*name).into()).collect(),
            upgraded: previous.is_some(),
        })
    })
}

fn reject_overlapping_roots(package_root: &Path, paths: &InstallPaths) -> Result<()> {
    let package_root = std::fs::canonicalize(package_root)?;
    for destination in [&paths.root, &paths.bin_dir, &paths.share_dir] {
        let existing = canonical_candidate(destination)?;
        if existing == package_root
            || existing.starts_with(&package_root)
            || package_root.starts_with(&existing)
        {
            return Err(Error::Conflict(format!(
                "package root and installation destination overlap: {}",
                destination.display()
            )));
        }
    }
    Ok(())
}

fn canonical_candidate(path: &Path) -> Result<PathBuf> {
    let mut cursor = path.to_path_buf();
    let mut tail = Vec::new();
    while !cursor.exists() {
        let name = cursor.file_name().ok_or_else(|| {
            Error::InvalidArgument(format!("invalid installation path {}", path.display()))
        })?;
        tail.push(name.to_owned());
        cursor = cursor
            .parent()
            .ok_or_else(|| {
                Error::InvalidArgument(format!("invalid installation path {}", path.display()))
            })?
            .to_path_buf();
    }
    let mut result = std::fs::canonicalize(cursor)?;
    for component in tail.iter().rev() {
        result.push(component);
    }
    Ok(result)
}

pub(crate) struct Stage {
    pub(crate) _root: TempDir,
    pub(crate) _guards: Vec<PathGuard>,
    pub(crate) paths: InstallPaths,
    pub(crate) manifest_path: PathBuf,
}

fn stage_package(package: &Package, destination: &InstallPaths) -> Result<Stage> {
    let root = tempfile::tempdir_in(&destination.root)?;
    let mut guards = vec![PathGuard::for_directory(root.path())?];
    let bin_dir = root.path().join("bin");
    let share_dir = root.path().join("share").join("neomax");
    guards.push(PathGuard::ensure_directory(&bin_dir)?);
    guards.push(PathGuard::ensure_directory(&share_dir)?);
    guards.push(PathGuard::ensure_directory(&share_dir.join("shell"))?);
    guards.push(PathGuard::ensure_directory(&share_dir.join("workflows"))?);
    guards.push(PathGuard::ensure_directory(&share_dir.join("agents"))?);
    let stage_paths = InstallPaths::new(root.path(), &bin_dir, &share_dir)?;
    copy_executable(
        &package.binary("neomax"),
        &stage_paths
            .bin_dir
            .join(super::package::binary_name("neomax")),
    )?;
    for alias in ALIASES.iter().skip(1) {
        let name = super::package::binary_name(alias);
        create_alias(
            &stage_paths
                .bin_dir
                .join(super::package::binary_name("neomax")),
            &stage_paths.bin_dir.join(name),
        )?;
    }
    for auxiliary in AUXILIARIES {
        let name = super::package::binary_name(auxiliary);
        copy_executable(&package.binary(auxiliary), &stage_paths.bin_dir.join(name))?;
    }
    for asset in ASSETS {
        copy_file(&package.asset(asset), &stage_paths.asset_path(asset))?;
    }
    for asset in SHELL_ASSETS {
        copy_file(&package.asset(asset), &stage_paths.asset_path(asset))?;
    }
    for doc in DOCS {
        copy_file(&package.doc(doc), &stage_paths.asset_path(doc))?;
    }
    for workflow in super::types::WORKFLOWS {
        copy_file(
            &package.workflow(workflow),
            &stage_paths.workflow_path(workflow),
        )?;
    }
    copy_file(
        &package.asset(KIMI_AGENT_ASSET),
        &stage_paths.asset_path(KIMI_AGENT_ASSET),
    )?;
    let manifest_path = stage_paths.manifest_path();
    Ok(Stage {
        _root: root,
        _guards: guards,
        paths: stage_paths,
        manifest_path,
    })
}

fn replacements(
    manifest: &InstallManifest,
    stage: &Stage,
    destination: &InstallPaths,
) -> Vec<Replacement> {
    let mut entries = manifest
        .files
        .iter()
        .map(|file| Replacement {
            source: installed_path(&stage.paths, &file.path),
            target: installed_path(destination, &file.path),
        })
        .collect::<Vec<_>>();
    entries.push(Replacement {
        source: stage.manifest_path.clone(),
        target: destination.manifest_path(),
    });
    entries
}

fn preflight_existing(
    next: &InstallManifest,
    previous: Option<&InstallManifest>,
    paths: &InstallPaths,
    force: bool,
) -> Result<()> {
    if path_exists(&paths.manifest_path()) && previous.is_none() && !force {
        return Err(Error::Conflict(format!(
            "refusing to replace an unrelated installation manifest {}; pass --force to replace it",
            paths.manifest_path().display()
        )));
    }
    for file in &next.files {
        let target = installed_path(paths, &file.path);
        if !path_exists(&target) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&target)?;
        match &file.kind {
            ManifestKind::File if !metadata.file_type().is_file() => {
                return Err(Error::Conflict(format!(
                    "refusing to replace a non-file installation path {}; pass --force only for a regular file",
                    target.display()
                )));
            }
            ManifestKind::Symlink if !metadata.file_type().is_symlink() => {
                return Err(Error::Conflict(format!(
                    "refusing to replace a non-link installation path {}; pass --force only for the Neomax link",
                    target.display()
                )));
            }
            _ => {}
        }
        if force {
            continue;
        }
        let owned = previous.and_then(|manifest| manifest.file(&file.path));
        match (owned, &file.kind) {
            (Some(old), ManifestKind::File) => {
                let expected = old.sha256.as_deref().ok_or_else(|| Error::InvalidState {
                    path: paths.manifest_path(),
                    message: format!("file {} has no recorded hash", file.path),
                })?;
                if !std::fs::symlink_metadata(&target)?.file_type().is_file() {
                    return Err(Error::Conflict(format!(
                        "refusing to replace modified installed file {}; pass --force to replace it",
                        target.display()
                    )));
                }
                if sha256(&target)? != expected {
                    return Err(Error::Conflict(format!(
                        "refusing to replace modified installed file {}; pass --force to replace it",
                        target.display()
                    )));
                }
            }
            (Some(_), ManifestKind::Symlink) => {
                if std::fs::read_link(&target).ok().as_deref() != Some(Path::new("neomax")) {
                    return Err(Error::Conflict(format!(
                        "refusing to replace modified installed alias {}; pass --force to replace it",
                        target.display()
                    )));
                }
            }
            (None, ManifestKind::Symlink) => {
                if std::fs::read_link(&target).ok().as_deref() != Some(Path::new("neomax")) {
                    return Err(Error::Conflict(format!(
                        "refusing to replace an unrelated file {}; pass --force to replace it",
                        target.display()
                    )));
                }
            }
            (None, ManifestKind::File) => {
                return Err(Error::Conflict(format!(
                    "refusing to replace an unrelated file {}; pass --force to replace it",
                    target.display()
                )));
            }
        }
    }
    Ok(())
}
