use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::transaction::{Mutation, snapshot_path};
use super::types::{FileSnapshot, same_profile};
use super::writer::CredentialWriter;

pub const AUTH_FILE: &str = "auth.json";

#[derive(Debug, Clone)]
pub struct CodexCopyPlan {
    pub destination: PathBuf,
    pub source: PathBuf,
    pub destination_before: FileSnapshot,
    pub mutations: Vec<Mutation>,
}

#[derive(Debug, Clone)]
pub struct CodexSwapPlan {
    pub first: PathBuf,
    pub second: PathBuf,
    pub first_before: FileSnapshot,
    pub second_before: FileSnapshot,
    pub mutations: Vec<Mutation>,
}

pub fn auth_path(profile: &Path) -> PathBuf {
    profile.join(AUTH_FILE)
}

pub fn prepare_copy<W: CredentialWriter>(
    writer: &W,
    destination: &Path,
    source: &Path,
) -> Result<CodexCopyPlan> {
    reject_same_profile(destination, source)?;
    let source_auth = required_auth(writer, source)?;
    let destination_before = read_snapshot(writer, destination)?;
    Ok(CodexCopyPlan {
        destination: destination.to_path_buf(),
        source: source.to_path_buf(),
        destination_before,
        mutations: vec![Mutation::write(auth_path(destination), source_auth)],
    })
}

pub fn prepare_swap<W: CredentialWriter>(
    writer: &W,
    first: &Path,
    second: &Path,
) -> Result<CodexSwapPlan> {
    reject_same_profile(first, second)?;
    let first_auth = required_auth(writer, first)?;
    let second_auth = required_auth(writer, second)?;
    let first_before = read_snapshot(writer, first)?;
    let second_before = read_snapshot(writer, second)?;
    Ok(CodexSwapPlan {
        first: first.to_path_buf(),
        second: second.to_path_buf(),
        first_before,
        second_before,
        mutations: vec![
            Mutation::write(auth_path(first), second_auth),
            Mutation::write(auth_path(second), first_auth),
        ],
    })
}

pub fn restore_mutations(snapshot: &FileSnapshot, profile: &Path) -> Vec<Mutation> {
    vec![match snapshot.auth.clone() {
        Some(bytes) => Mutation::write(auth_path(profile), bytes),
        None => Mutation::remove(auth_path(profile)),
    }]
}

pub(crate) fn read_snapshot<W: CredentialWriter>(
    writer: &W,
    profile: &Path,
) -> Result<FileSnapshot> {
    let state = snapshot_path(writer, &auth_path(profile))?;
    Ok(FileSnapshot {
        auth: state.bytes,
        ..FileSnapshot::default()
    })
}

fn required_auth<W: CredentialWriter>(writer: &W, profile: &Path) -> Result<Vec<u8>> {
    let Some(bytes) = writer.read_optional(&auth_path(profile))? else {
        return Err(Error::NotFound(format!(
            "Codex auth.json is missing for profile {}",
            profile.display()
        )));
    };
    if bytes.is_empty() {
        return Err(Error::InvalidArgument(format!(
            "Codex auth.json is empty for profile {}",
            profile.display()
        )));
    }
    Ok(bytes)
}

fn reject_same_profile(destination: &Path, source: &Path) -> Result<()> {
    if same_profile(destination, source) {
        return Err(Error::Conflict(
            "destination and source are the same profile".into(),
        ));
    }
    Ok(())
}
