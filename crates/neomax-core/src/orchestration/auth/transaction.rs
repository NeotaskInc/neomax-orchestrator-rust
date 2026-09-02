use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::writer::CredentialWriter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutation {
    pub path: PathBuf,
    pub bytes: Option<Vec<u8>>,
}

impl Mutation {
    pub fn write(path: impl Into<PathBuf>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            bytes: Some(bytes.into()),
        }
    }

    pub fn remove(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            bytes: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub path: PathBuf,
    pub bytes: Option<Vec<u8>>,
}

pub fn snapshot_path<W: CredentialWriter>(writer: &W, path: &Path) -> Result<FileState> {
    Ok(FileState {
        path: path.to_path_buf(),
        bytes: writer.read_optional(path)?,
    })
}

pub fn snapshot_paths<W: CredentialWriter>(
    writer: &W,
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<Vec<FileState>> {
    paths
        .into_iter()
        .map(|path| snapshot_path(writer, &path))
        .collect()
}

pub fn apply_with_rollback<W: CredentialWriter>(
    writer: &W,
    mutations: &[Mutation],
    snapshots: &[FileState],
) -> Result<()> {
    for mutation in mutations {
        let result = match mutation.bytes.as_deref() {
            Some(bytes) => writer.write_atomic(&mutation.path, bytes),
            None => writer.remove(&mutation.path),
        };
        if let Err(error) = result {
            return rollback_after_failure(writer, snapshots, error);
        }
    }
    Ok(())
}

fn rollback_after_failure<W: CredentialWriter>(
    writer: &W,
    snapshots: &[FileState],
    original: Error,
) -> Result<()> {
    let mut rollback_error = None;
    for state in snapshots.iter().rev() {
        let result = match state.bytes.as_deref() {
            Some(bytes) => writer.write_atomic(&state.path, bytes),
            None => writer.remove(&state.path),
        };
        if let Err(error) = result {
            rollback_error = Some(error);
            break;
        }
    }
    match rollback_error {
        Some(error) => Err(Error::Message(format!(
            "credential mutation failed and rollback failed: {original}; rollback: {error}"
        ))),
        None => Err(original),
    }
}
