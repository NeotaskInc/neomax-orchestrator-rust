use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::platform::{
    apply_permissions, create_symlink, is_cross_device, open_regular_source, RenameOps,
};
use super::state::{Backup, BackupKind};
use super::validation::{source_kind, SourceKind};

pub(super) fn stage_backup<R: RenameOps>(
    target: &Path,
    backup: &Path,
    renamer: &R,
) -> Result<Backup> {
    match renamer.rename(target, backup) {
        Ok(()) => Ok(Backup {
            path: backup.to_path_buf(),
            kind: BackupKind::Moved,
        }),
        Err(error) if is_cross_device(&error) => {
            copy_entry_to(target, backup, renamer)?;
            Ok(Backup {
                path: backup.to_path_buf(),
                kind: BackupKind::Copied,
            })
        }
        Err(error) => Err(Error::Io(error)),
    }
}

pub(super) fn copy_entry_to<R: RenameOps>(
    source: &Path,
    target: &Path,
    renamer: &R,
) -> Result<()> {
    let parent = target.parent().ok_or_else(|| {
        Error::InvalidArgument(format!(
            "installation target has no parent: {}",
            target.display()
        ))
    })?;
    let _target_guard = crate::io::PathGuard::ensure_directory(parent)?;
    let staged = copy_entry_to_temp(source, parent)?;
    if let Err(error) = renamer.rename(&staged, target) {
        let _ = fs::remove_file(&staged);
        return Err(Error::Io(error));
    }
    Ok(())
}

fn copy_entry_to_temp(source: &Path, parent: &Path) -> Result<PathBuf> {
    match source_kind(source)? {
        SourceKind::File(permissions) => {
            let mut input = open_regular_source(source)?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
            io::copy(&mut input, temporary.as_file_mut())?;
            apply_permissions(temporary.as_file(), permissions)?;
            temporary.as_file().sync_all()?;
            let (_, path) = temporary.keep().map_err(|error| Error::Io(error.error))?;
            Ok(path)
        }
        SourceKind::Symlink(link) => {
            let temporary = tempfile::NamedTempFile::new_in(parent)?;
            let path = temporary.path().to_path_buf();
            temporary.close()?;
            create_symlink(&link, &path)?;
            Ok(path)
        }
    }
}
