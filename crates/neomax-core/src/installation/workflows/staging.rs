use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::io::{PathGuard, path_to_string};
use crate::{Engine, Error, Result};

use super::super::files::{path_exists, sha256};
use super::super::package::Package;
use super::super::paths::InstallPaths;
use super::super::transaction::Replacement;
use super::super::types::{KIMI_AGENT_ASSET, KIMI_AGENT_RECORD, WORKFLOWS};
use super::hooks::{merge_hooks, read_settings, write_json_stage_private};
use super::manifest::{HookRecord, WorkflowFile, WorkflowManifest, WorkflowStage};
use super::support::profile_home;
use super::targets::{
    discover_profiles, kimi_agent_target, read_kimi_agent_source, read_kimi_agent_source_from_path,
    read_source, render_workflow, workflow_target,
};

pub(crate) fn stage(
    package: &Package,
    install_stage: &super::super::install::Stage,
    paths: &InstallPaths,
    previous: Option<&WorkflowManifest>,
    force: bool,
    profile_home_override: Option<&Path>,
) -> Result<WorkflowStage> {
    let home = profile_home_override
        .map(Path::to_path_buf)
        .map_or_else(profile_home, Ok)?;
    let allow_process_environment = profile_home_override.is_none();
    if !home.is_absolute() {
        return Err(Error::InvalidArgument(
            "workflow profile home must be an absolute path".into(),
        ));
    }
    let profiles = discover_profiles(&home);
    let previous = previous.cloned().unwrap_or_else(WorkflowManifest::empty);
    let mut manifest = WorkflowManifest::empty();
    manifest.home = path_to_string("workflow home", &home)?;
    let mut replacements = Vec::new();
    let mut guards = Vec::new();
    let stage_dir = install_stage.paths.root.join("workflow-files");
    guards.push(PathGuard::ensure_directory(&stage_dir)?);
    let mut index = 0_u64;
    let mut targets = BTreeSet::new();

    for engine in Engine::ALL {
        for profile in profiles.get(&engine).into_iter().flatten() {
            for workflow in WORKFLOWS {
                let target = workflow_target(
                    engine,
                    profile,
                    workflow,
                    &home,
                    allow_process_environment,
                );
                if !targets.insert(target.clone()) {
                    continue;
                }
                if let Some(parent) = target.parent() {
                    guards.push(PathGuard::ensure_directory(parent)?);
                }
                let content = render_workflow(engine, workflow, &read_source(package, workflow)?)?;
                let source = stage_dir.join(format!("{index}.md"));
                index += 1;
                fs::write(&source, content.as_bytes())?;
                preflight_workflow_target(&target, engine, workflow, &previous, force)?;
                let hash = sha256(&source)?;
                replacements.push(Replacement {
                    source,
                    target: target.clone(),
                });
                manifest.files.push(WorkflowFile {
                    path: path_to_string("workflow target", &target)?,
                    engine,
                    workflow: (*workflow).into(),
                    sha256: hash,
                });
            }
            if engine == Engine::Kimi {
                let target = kimi_agent_target(profile);
                if targets.insert(target.clone()) {
                    if let Some(parent) = target.parent() {
                        guards.push(PathGuard::ensure_directory(parent)?);
                    }
                    let content = read_kimi_agent_source(package)?;
                    let source = stage_dir.join(format!("{index}.md"));
                    index += 1;
                    fs::write(&source, content.as_bytes())?;
                    preflight_workflow_target(
                        &target,
                        Engine::Kimi,
                        KIMI_AGENT_RECORD,
                        &previous,
                        force,
                    )?;
                    let hash = sha256(&source)?;
                    replacements.push(Replacement {
                        source,
                        target: target.clone(),
                    });
                    manifest.files.push(WorkflowFile {
                        path: path_to_string("workflow target", &target)?,
                        engine: Engine::Kimi,
                        workflow: KIMI_AGENT_RECORD.into(),
                        sha256: hash,
                    });
                }
            }
        }
    }

    for profile in profiles.get(&Engine::Claude).into_iter().flatten() {
        let target = profile.join("settings.json");
        if !targets.insert(target.clone()) {
            continue;
        }
        if let Some(parent) = target.parent() {
            guards.push(PathGuard::ensure_directory(parent)?);
        }
        let existing = read_settings(&target)?;
        let bin = paths
            .bin_dir
            .join(super::super::package::binary_name("neomax"));
        let (settings, commands) = merge_hooks(existing, &bin)?;
        let source = stage_dir.join(format!("{index}.json"));
        index += 1;
        write_json_stage_private(&source, &settings)?;
        preflight_settings_target(&target)?;
        replacements.push(Replacement {
            source: source.clone(),
            target: target.clone(),
        });
        for (event, command) in commands {
            manifest.hooks.push(HookRecord {
                path: path_to_string("Claude settings path", &target)?,
                event,
                command,
            });
        }
    }

    for file in &previous.files {
        if !manifest
            .files
            .iter()
            .any(|current| current.path == file.path)
        {
            manifest.files.push(file.clone());
        }
    }
    for hook in &previous.hooks {
        if !manifest.hooks.contains(hook) {
            manifest.hooks.push(hook.clone());
        }
    }

    let manifest_source = stage_dir.join("manifest.json");
    write_json_stage_private(&manifest_source, &manifest)?;
    replacements.push(Replacement {
        source: manifest_source,
        target: paths.workflow_manifest_path(),
    });
    guards.push(PathGuard::for_path(&paths.workflow_manifest_path())?);
    Ok(WorkflowStage {
        replacements,
        _guards: guards,
    })
}

pub fn ensure_profile_workflows(engine: Engine, profile: &Path, home: &Path) -> Result<()> {
    if !home.is_absolute() || !profile.is_absolute() {
        return Err(Error::InvalidArgument(
            "provider profile must be an absolute path within the user home".into(),
        ));
    }
    let resolved_home = resolve_for_containment(home, "user home")?;
    let resolved_profile = resolve_for_containment(profile, "provider profile")?;
    if !resolved_profile.starts_with(&resolved_home) {
        return Err(Error::InvalidArgument(
            "provider profile must be an absolute path within the user home".into(),
        ));
    }
    let install_paths = InstallPaths::discover()?;
    ensure_profile_workflows_at(engine, profile, home, &install_paths)
}

fn resolve_for_containment(path: &Path, label: &str) -> Result<std::path::PathBuf> {
    let mut missing = Vec::new();
    let mut existing = path.to_path_buf();
    loop {
        match fs::symlink_metadata(&existing) {
            Ok(_) => {
                let mut resolved = fs::canonicalize(&existing).map_err(|error| {
                    Error::InvalidArgument(format!(
                        "could not resolve {label}: {} ({error})",
                        existing.display()
                    ))
                })?;
                for component in missing.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    Error::InvalidArgument(format!("{label} has no usable directory name"))
                })?;
                missing.push(name.to_os_string());
                existing = existing
                    .parent()
                    .ok_or_else(|| {
                        Error::InvalidArgument(format!("{label} has no existing parent"))
                    })?
                    .to_path_buf();
            }
            Err(error) => {
                return Err(Error::InvalidArgument(format!(
                    "could not inspect {label}: {} ({error})",
                    existing.display()
                )));
            }
        }
    }
}

pub(crate) fn ensure_profile_workflows_at(
    engine: Engine,
    profile: &Path,
    home: &Path,
    install_paths: &InstallPaths,
) -> Result<()> {
    let source_root = install_paths.share_dir.join("workflows");
    if !source_root.is_dir() {
        return Ok(());
    }
    let lock = install_paths.lock_path.clone();
    crate::atomic::with_exclusive_lock(&lock, || {
        let previous =
            WorkflowManifest::read(install_paths)?.unwrap_or_else(WorkflowManifest::empty);
        let mut manifest = previous.clone();
        manifest.home = path_to_string("workflow home", home)?;
        let stage = tempfile::tempdir_in(home)?;
        let mut guards = vec![
            PathGuard::for_directory(home)?,
            PathGuard::for_directory(stage.path())?,
            PathGuard::for_directory(&source_root)?,
        ];
        let mut replacements = Vec::new();
        let mut index = 0_u64;
        for workflow in WORKFLOWS {
            let target = workflow_target(engine, profile, workflow, home, true);
            if let Some(parent) = target.parent() {
                guards.push(PathGuard::ensure_directory(parent)?);
            }
            let target_path = path_to_string("workflow target", &target)?;
            if path_exists(&target) {
                continue;
            }
            let source = source_root.join(workflow);
            if !source.is_file() {
                continue;
            }
            let content = render_workflow(
                engine,
                workflow,
                &String::from_utf8_lossy(&super::super::files::read_bounded(
                    &source,
                    super::targets::MAX_WORKFLOW_SOURCE_BYTES,
                )?),
            )?;
            let staged = stage.path().join(format!("workflow-{index}.md"));
            index += 1;
            fs::write(&staged, content.as_bytes())?;
            manifest.files.retain(|file| file.path != target_path);
            manifest.files.push(WorkflowFile {
                path: target_path,
                engine,
                workflow: (*workflow).into(),
                sha256: sha256(&staged)?,
            });
            replacements.push(Replacement {
                source: staged,
                target,
            });
        }
        if engine == Engine::Kimi {
            let target = kimi_agent_target(profile);
            if let Some(parent) = target.parent() {
                guards.push(PathGuard::ensure_directory(parent)?);
            }
            if !path_exists(&target) {
                let source = source_root
                    .parent()
                    .unwrap_or(source_root.as_path())
                    .join(KIMI_AGENT_ASSET);
                if source.is_file() {
                    let content = read_kimi_agent_source_from_path(&source)?;
                    let staged = stage.path().join(format!("workflow-{index}.md"));
                    index += 1;
                    fs::write(&staged, content.as_bytes())?;
                    let target_path = path_to_string("Kimi agent path", &target)?;
                    manifest.files.retain(|file| file.path != target_path);
                    manifest.files.push(WorkflowFile {
                        path: target_path,
                        engine: Engine::Kimi,
                        workflow: KIMI_AGENT_RECORD.into(),
                        sha256: sha256(&staged)?,
                    });
                    replacements.push(Replacement {
                        source: staged,
                        target,
                    });
                }
            }
        }
        if engine == Engine::Claude {
            let settings_path = profile.join("settings.json");
            if let Some(parent) = settings_path.parent() {
                guards.push(PathGuard::ensure_directory(parent)?);
            }
            preflight_settings_target(&settings_path)?;
            let (settings, commands) = merge_hooks(
                read_settings(&settings_path)?,
                &install_paths
                    .bin_dir
                    .join(super::super::package::binary_name("neomax")),
            )?;
            let staged = stage.path().join(format!("settings-{index}.json"));
            write_json_stage_private(&staged, &settings)?;
            replacements.push(Replacement {
                source: staged,
                target: settings_path.clone(),
            });
            let settings_path = path_to_string("Claude settings path", &settings_path)?;
            for (event, command) in commands {
                let record = HookRecord {
                    path: settings_path.clone(),
                    event,
                    command,
                };
                if !manifest.hooks.contains(&record) {
                    manifest.hooks.push(record);
                }
            }
        }
        let manifest_source = stage.path().join("manifest.json");
        write_json_stage_private(&manifest_source, &manifest)?;
        replacements.push(Replacement {
            source: manifest_source,
            target: install_paths.workflow_manifest_path(),
        });
        guards.push(PathGuard::for_path(&install_paths.workflow_manifest_path())?);
        super::super::transaction::replace_all(
            &replacements,
            home.parent().unwrap_or(Path::new(".")),
        )
    })
}

fn preflight_workflow_target(
    target: &Path,
    engine: Engine,
    workflow: &str,
    previous: &WorkflowManifest,
    force: bool,
) -> Result<()> {
    let target_path = path_to_string("workflow target", target)?;
    if !path_exists(target) {
        return Ok(());
    }
    if !fs::symlink_metadata(target)?.file_type().is_file() {
        return Err(Error::Conflict(format!(
            "refusing to replace non-file workflow target {}",
            target.display()
        )));
    }
    if force {
        return Ok(());
    }
    let Some(old) = previous.files.iter().find(|file| {
        file.path == target_path && file.engine == engine && file.workflow == workflow
    }) else {
        return Err(Error::Conflict(format!(
            "refusing to replace unrelated workflow {}; pass --force to replace it",
            target.display()
        )));
    };
    if sha256(target)? != old.sha256 {
        return Err(Error::Conflict(format!(
            "refusing to replace modified workflow {}; pass --force to replace it",
            target.display()
        )));
    }
    Ok(())
}

fn preflight_settings_target(target: &Path) -> Result<()> {
    if path_exists(target) && !fs::symlink_metadata(target)?.file_type().is_file() {
        return Err(Error::Conflict(format!(
            "refusing to replace non-file Claude settings {}",
            target.display()
        )));
    }
    Ok(())
}
