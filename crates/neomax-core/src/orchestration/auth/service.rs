use std::path::Path;

use crate::{Engine, Result};

use super::backup::BackupStore;
use super::claude;
use super::codex;
use super::policy::copy_allowed;
use super::restore;
use super::rotation_log::{RotationEvent, RotationEventContext, RotationLog};
use super::transaction::{FileState, apply_with_rollback};
use super::types::{FileSnapshot, RotationEffects, RotationOperation, RotationPaths};
use super::writer::{CredentialWriter, FsCredentialWriter};

pub struct RotationService<W = FsCredentialWriter> {
    writer: W,
    backups: BackupStore,
    log: RotationLog,
    paths: RotationPaths,
}

impl RotationService<FsCredentialWriter> {
    pub fn filesystem(paths: RotationPaths) -> Self {
        Self::new(FsCredentialWriter, paths)
    }
}

impl<W: CredentialWriter> RotationService<W> {
    pub fn new(writer: W, paths: RotationPaths) -> Self {
        let backups = BackupStore::new(paths.backup_dir.clone());
        let log = RotationLog::new(paths.rotation_log.clone());
        Self {
            writer,
            backups,
            log,
            paths,
        }
    }

    pub fn copy(
        &self,
        engine: Engine,
        destination: &Path,
        source: &Path,
        timestamp: i64,
        reason: Option<String>,
    ) -> Result<RotationEffects> {
        copy_allowed(engine)?;
        match engine {
            Engine::Claude => {
                let plan = claude::prepare_copy(&self.writer, destination, source)?;
                let backup =
                    self.backups
                        .save(engine, destination, &plan.destination_before, timestamp)?;
                let snapshots = claude_states(destination, &plan.destination_before);
                apply_with_rollback(&self.writer, &plan.mutations, &snapshots)?;
                let effects = RotationEffects::for_profile(
                    engine,
                    RotationOperation::Copy,
                    destination,
                    Some(source.to_path_buf()),
                    vec![backup],
                    self.paths.usage_cache_dir.as_deref(),
                    Vec::new(),
                );
                self.log
                    .append(&RotationEvent::from_context(RotationEventContext {
                        ts: timestamp,
                        engine,
                        operation: "copy",
                        destination,
                        source: Some(source),
                        from_email: plan.from_email,
                        to_email: plan.to_email,
                        reason,
                    }))?;
                Ok(effects)
            }
            Engine::Codex => {
                let plan = codex::prepare_copy(&self.writer, destination, source)?;
                let backup =
                    self.backups
                        .save(engine, destination, &plan.destination_before, timestamp)?;
                let snapshots = codex_states(destination, &plan.destination_before);
                apply_with_rollback(&self.writer, &plan.mutations, &snapshots)?;
                let effects = RotationEffects::for_profile(
                    engine,
                    RotationOperation::Copy,
                    destination,
                    Some(source.to_path_buf()),
                    vec![backup],
                    self.paths.usage_cache_dir.as_deref(),
                    Vec::new(),
                );
                self.log
                    .append(&RotationEvent::from_context(RotationEventContext {
                        ts: timestamp,
                        engine,
                        operation: "copy",
                        destination,
                        source: Some(source),
                        from_email: None,
                        to_email: None,
                        reason,
                    }))?;
                Ok(effects)
            }
            _ => unreachable!("provider restriction is checked above"),
        }
    }

    pub fn swap(
        &self,
        engine: Engine,
        first: &Path,
        second: &Path,
        timestamp: i64,
        reason: Option<String>,
    ) -> Result<RotationEffects> {
        copy_allowed(engine)?;
        match engine {
            Engine::Claude => {
                let plan = claude::prepare_swap(&self.writer, first, second)?;
                let first_backup =
                    self.backups
                        .save(engine, first, &plan.first_before, timestamp)?;
                let second_backup =
                    self.backups
                        .save(engine, second, &plan.second_before, timestamp)?;
                let snapshots = [
                    claude_states(first, &plan.first_before),
                    claude_states(second, &plan.second_before),
                ]
                .concat();
                apply_with_rollback(&self.writer, &plan.mutations, &snapshots)?;
                let effects = RotationEffects::for_profile(
                    engine,
                    RotationOperation::Swap,
                    first,
                    Some(second.to_path_buf()),
                    vec![first_backup, second_backup],
                    self.paths.usage_cache_dir.as_deref(),
                    [second.to_path_buf()],
                );
                self.log
                    .append(&RotationEvent::from_context(RotationEventContext {
                        ts: timestamp,
                        engine,
                        operation: "swap",
                        destination: first,
                        source: Some(second),
                        from_email: plan.first_email,
                        to_email: plan.second_email,
                        reason,
                    }))?;
                Ok(effects)
            }
            Engine::Codex => {
                let plan = codex::prepare_swap(&self.writer, first, second)?;
                let first_backup =
                    self.backups
                        .save(engine, first, &plan.first_before, timestamp)?;
                let second_backup =
                    self.backups
                        .save(engine, second, &plan.second_before, timestamp)?;
                let snapshots = [
                    codex_states(first, &plan.first_before),
                    codex_states(second, &plan.second_before),
                ]
                .concat();
                apply_with_rollback(&self.writer, &plan.mutations, &snapshots)?;
                let effects = RotationEffects::for_profile(
                    engine,
                    RotationOperation::Swap,
                    first,
                    Some(second.to_path_buf()),
                    vec![first_backup, second_backup],
                    self.paths.usage_cache_dir.as_deref(),
                    [second.to_path_buf()],
                );
                self.log
                    .append(&RotationEvent::from_context(RotationEventContext {
                        ts: timestamp,
                        engine,
                        operation: "swap",
                        destination: first,
                        source: Some(second),
                        from_email: None,
                        to_email: None,
                        reason,
                    }))?;
                Ok(effects)
            }
            _ => unreachable!("provider restriction is checked above"),
        }
    }

    pub fn restore(
        &self,
        engine: Engine,
        destination: &Path,
        backup: Option<&Path>,
        timestamp: i64,
        reason: Option<String>,
    ) -> Result<RotationEffects> {
        copy_allowed(engine)?;
        let plan = restore::prepare(&self.writer, &self.backups, engine, destination, backup)?;
        let safety_backup =
            self.backups
                .save_safety(engine, destination, &plan.current, timestamp)?;
        restore::apply(&self.writer, &plan)?;
        let effects = RotationEffects::for_profile(
            engine,
            RotationOperation::Restore,
            destination,
            None,
            vec![safety_backup],
            self.paths.usage_cache_dir.as_deref(),
            Vec::new(),
        );
        self.log
            .append(&RotationEvent::from_context(RotationEventContext {
                ts: timestamp,
                engine,
                operation: "restore",
                destination,
                source: None,
                from_email: None,
                to_email: plan.to_email,
                reason,
            }))?;
        Ok(effects)
    }

    pub fn recent_rotations(&self, limit: usize) -> Result<Vec<RotationEvent>> {
        self.log.recent(limit)
    }
}

fn claude_states(profile: &Path, snapshot: &FileSnapshot) -> Vec<FileState> {
    vec![
        FileState {
            path: claude::credential_path(profile),
            bytes: snapshot.credential.clone(),
        },
        FileState {
            path: claude::identity_path(profile),
            bytes: snapshot.identity.clone(),
        },
    ]
}

fn codex_states(profile: &Path, snapshot: &FileSnapshot) -> Vec<FileState> {
    vec![FileState {
        path: codex::auth_path(profile),
        bytes: snapshot.auth.clone(),
    }]
}
