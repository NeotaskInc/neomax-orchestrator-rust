mod activation;
mod platform;
mod rollback;
mod staging;
mod state;
mod validation;

use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};
use crate::io::PathGuard;

use super::files::path_exists;
use activation::move_with_fallback;
use platform::{sync_transaction_paths, RenameOps, SystemRename};
#[cfg(test)]
use platform::system_rename;
use rollback::{rollback, with_rollback_error};
use staging::stage_backup;
use state::{Moved, TransactionState};
use validation::{validate_removals, validate_replacements};

#[derive(Debug, Clone)]
pub(crate) struct Replacement {
    pub source: PathBuf,
    pub target: PathBuf,
}

pub(crate) fn replace_all(entries: &[Replacement], backup_parent: &Path) -> Result<()> {
    replace_all_with(entries, backup_parent, &SystemRename)
}

fn replace_all_with<R: RenameOps>(
    entries: &[Replacement],
    backup_parent: &Path,
    renamer: &R,
) -> Result<()> {
    let mut path_guards = vec![PathGuard::ensure_directory(backup_parent)?];
    for entry in entries {
        path_guards.push(PathGuard::for_path(&entry.source)?);
        if let Some(parent) = entry.target.parent() {
            path_guards.push(PathGuard::ensure_directory(parent)?);
        }
    }
    validate_replacements(entries)?;
    let backup_dir = tempfile::tempdir_in(backup_parent)?;
    path_guards.push(PathGuard::ensure_directory(backup_dir.path())?);
    let mut state = TransactionState::default();
    let result = (|| {
        for (index, entry) in entries.iter().enumerate() {
            let backup = if path_exists(&entry.target) {
                let path = backup_dir.path().join(index.to_string());
                Some(
                    stage_backup(&entry.target, &path, renamer).map_err(|error| {
                        Error::Message(format!(
                            "could not stage existing installation file {}: {error}",
                            entry.target.display()
                        ))
                    })?,
                )
            } else {
                None
            };
            state.push(Moved {
                target: entry.target.clone(),
                backup,
                activated: false,
            });
            sync_transaction_paths(
                std::iter::once(entry.target.as_path()).chain(state.last_backup_path()),
            )?;
            move_with_fallback(&entry.source, &entry.target, renamer).map_err(|error| {
                Error::Message(format!(
                    "could not activate installation file {}: {error}",
                    entry.target.display()
                ))
            })?;
            state.mark_last_activated();
            sync_transaction_paths([entry.source.as_path(), entry.target.as_path()])?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            drop(backup_dir);
            Ok(())
        }
        Err(error) => Err(with_rollback_error(error, rollback(state.entries(), renamer))),
    }
}

pub(crate) fn remove_all(targets: &[PathBuf], backup_parent: &Path) -> Result<()> {
    remove_all_with(targets, backup_parent, &SystemRename)
}

fn remove_all_with<R: RenameOps>(
    targets: &[PathBuf],
    backup_parent: &Path,
    renamer: &R,
) -> Result<()> {
    let mut path_guards = vec![PathGuard::ensure_directory(backup_parent)?];
    for target in targets {
        path_guards.push(PathGuard::for_path(target)?);
    }
    validate_removals(targets)?;
    let backup_dir = tempfile::tempdir_in(backup_parent)?;
    path_guards.push(PathGuard::ensure_directory(backup_dir.path())?);
    let mut state = TransactionState::default();
    let result = (|| {
        for (index, target) in targets.iter().enumerate() {
            if !path_exists(target) {
                continue;
            }
            let backup_path = backup_dir.path().join(index.to_string());
            let backup = stage_backup(target, &backup_path, renamer).map_err(|error| {
                Error::Message(format!(
                    "could not stage installation file {} for removal: {error}",
                    target.display()
                ))
            })?;
            let backup_kind = backup.kind;
            state.push(Moved {
                target: target.clone(),
                backup: Some(backup),
                activated: backup_kind == state::BackupKind::Moved,
            });
            sync_transaction_paths(
                std::iter::once(target.as_path()).chain(state.last_backup_path()),
            )?;
            if backup_kind == state::BackupKind::Copied {
                fs::remove_file(target).map_err(|error| {
                    Error::Message(format!(
                        "could not remove installation file {}: {error}",
                        target.display()
                    ))
                })?;
                state.mark_last_activated();
                sync_transaction_paths([target.as_path()])?;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            drop(backup_dir);
            Ok(())
        }
        Err(error) => Err(with_rollback_error(error, rollback(state.entries(), renamer))),
    }
}

#[cfg(test)]
#[path = "transaction_tests.rs"]
mod tests;
