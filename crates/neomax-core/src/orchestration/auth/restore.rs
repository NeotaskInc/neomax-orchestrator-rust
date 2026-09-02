use std::path::{Path, PathBuf};

use crate::{Engine, Error, Result};

use super::backup::{BackupDocument, BackupStore};
use super::claude;
use super::codex;
use super::transaction::{apply_with_rollback, FileState};
use super::types::{absolute_path, FileSnapshot};
use super::writer::CredentialWriter;

#[derive(Debug, Clone)]
pub struct RestorePlan {
    pub engine: Engine,
    pub profile: PathBuf,
    pub backup: PathBuf,
    pub current: FileSnapshot,
    pub target: FileSnapshot,
    pub mutations: Vec<super::transaction::Mutation>,
    pub to_email: Option<String>,
}

pub fn prepare<W: CredentialWriter>(
    writer: &W,
    backups: &BackupStore,
    engine: Engine,
    profile: &Path,
    backup_path: Option<&Path>,
) -> Result<RestorePlan> {
    let (backup, document) = match backup_path {
        Some(path) => (path.to_path_buf(), backups.load_for_profile(path, profile)?),
        None => backups.latest(engine, profile)?.ok_or_else(|| {
            Error::NotFound(format!(
                "no credential backup for profile {}",
                profile.display()
            ))
        })?,
    };
    validate_document(&document, engine, profile, &backup)?;
    let target = document.snapshot()?;
    let current = match engine {
        Engine::Claude => claude::read_snapshot(writer, profile)?,
        Engine::Codex => codex::read_snapshot(writer, profile)?,
        _ => unreachable!("provider restriction is checked by the service"),
    };
    let mutations = match engine {
        Engine::Claude => claude::restore_mutations(&target, profile),
        Engine::Codex => codex::restore_mutations(&target, profile),
        _ => Vec::new(),
    };
    let to_email = (engine == Engine::Claude)
        .then(|| claude::account_email(&target))
        .flatten();
    Ok(RestorePlan {
        engine,
        profile: profile.to_path_buf(),
        backup,
        current,
        target,
        mutations,
        to_email,
    })
}

pub fn apply<W: CredentialWriter>(writer: &W, plan: &RestorePlan) -> Result<()> {
    let snapshots = snapshot_states(plan.engine, &plan.current, plan.profile.as_path());
    apply_with_rollback(writer, &plan.mutations, &snapshots)
}

fn snapshot_states(engine: Engine, snapshot: &FileSnapshot, profile: &Path) -> Vec<FileState> {
    match engine {
        Engine::Claude => vec![
            FileState {
                path: claude::credential_path(profile),
                bytes: snapshot.credential.clone(),
            },
            FileState {
                path: claude::identity_path(profile),
                bytes: snapshot.identity.clone(),
            },
        ],
        Engine::Codex => vec![FileState {
            path: codex::auth_path(profile),
            bytes: snapshot.auth.clone(),
        }],
        _ => Vec::new(),
    }
}

fn validate_document(
    document: &BackupDocument,
    engine: Engine,
    profile: &Path,
    backup: &Path,
) -> Result<()> {
    if document.engine != engine {
        return Err(Error::InvalidArgument(format!(
            "backup {} belongs to {}, not {}",
            backup.display(),
            document.engine,
            engine
        )));
    }
    if absolute_path(&document.profile) != absolute_path(profile) {
        return Err(Error::InvalidArgument(format!(
            "backup {} belongs to another profile",
            backup.display()
        )));
    }
    Ok(())
}
