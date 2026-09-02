use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::Result;

use super::super::files::path_exists;
use super::super::super::Error;
use super::super::transaction::Replacement;

#[derive(Debug)]
pub(super) enum SourceKind {
    File(fs::Permissions),
    Symlink(PathBuf),
}

pub(super) fn validate_replacements(entries: &[Replacement]) -> Result<()> {
    let mut targets = BTreeSet::new();
    for entry in entries {
        if entry.source == entry.target {
            return Err(Error::Conflict(format!(
                "installation source and target are identical: {}",
                entry.target.display()
            )));
        }
        source_kind(&entry.source).map_err(|error| {
            Error::Message(format!(
                "could not activate installation file {}: {error}",
                entry.target.display()
            ))
        })?;
        if path_exists(&entry.target) {
            ensure_replaceable(&entry.target)?;
        }
        if !targets.insert(entry.target.clone()) {
            return Err(Error::Conflict(format!(
                "installation target appears more than once: {}",
                entry.target.display()
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_removals(targets: &[PathBuf]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for target in targets {
        if !seen.insert(target.clone()) {
            return Err(Error::Conflict(format!(
                "installation target appears more than once: {}",
                target.display()
            )));
        }
        if path_exists(target) {
            ensure_replaceable(target)?;
        }
    }
    Ok(())
}

fn ensure_replaceable(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    if is_reparse_point(&metadata) {
        return Err(Error::Conflict(format!(
            "installation transaction refuses a reparse point: {}",
            path.display()
        )));
    }
    if metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Ok(());
    }
    Err(Error::Conflict(format!(
        "installation transaction only supports regular files and symbolic links: {}",
        path.display()
    )))
}

pub(super) fn source_kind(path: &Path) -> Result<SourceKind> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    if is_reparse_point(&metadata) {
        return Err(Error::Conflict(format!(
            "installation transaction refuses a reparse point source: {}",
            path.display()
        )));
    }
    if metadata.file_type().is_file() {
        return Ok(SourceKind::File(metadata.permissions()));
    }
    if metadata.file_type().is_symlink() {
        return Ok(SourceKind::Symlink(fs::read_link(path)?));
    }
    Err(Error::Conflict(format!(
        "installation transaction source is not a regular file or symbolic link: {}",
        path.display()
    )))
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}
