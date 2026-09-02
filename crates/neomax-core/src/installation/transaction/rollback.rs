use std::fs;
use std::io;
use std::path::Path;

use crate::{Error, Result};

use super::activation::move_with_fallback;
use super::platform::{sync_transaction_paths, RenameOps};
use super::state::{BackupKind, Moved};

pub(super) fn rollback<R: RenameOps>(moved: &[Moved], renamer: &R) -> Result<()> {
    let mut failures = Vec::new();
    for item in moved.iter().rev() {
        let Some(backup) = &item.backup else {
            if item.activated {
                remove_activated_target(&item.target, &mut failures);
            }
            continue;
        };
        let restore_needed = backup.kind == BackupKind::Moved || item.activated;
        if item.activated {
            remove_activated_target(&item.target, &mut failures);
        }
        if !restore_needed {
            continue;
        }
        if !super::super::files::path_exists(&backup.path) {
            failures.push(format!(
                "backup disappeared before rollback: {}",
                backup.path.display()
            ));
            continue;
        }
        if let Err(error) = move_with_fallback(&backup.path, &item.target, renamer)
            .and_then(|()| sync_transaction_paths([backup.path.as_path(), item.target.as_path()]))
        {
            failures.push(format!(
                "could not restore {} from {}: {error}",
                item.target.display(),
                backup.path.display()
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(Error::Message(failures.join("; ")))
    }
}

fn remove_activated_target(target: &Path, failures: &mut Vec<String>) {
    match fs::remove_file(target) {
        Ok(()) => {
            if let Err(error) = sync_transaction_paths([target]) {
                failures.push(format!(
                    "could not sync removed installation target {}: {error}",
                    target.display()
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => failures.push(format!(
            "could not remove activated installation target {}: {error}",
            target.display()
        )),
    }
}

pub(super) fn with_rollback_error(error: Error, rollback: Result<()>) -> Error {
    match rollback {
        Ok(()) => error,
        Err(rollback_error) => Error::Message(format!(
            "{error}; rollback failed: {rollback_error}"
        )),
    }
}
