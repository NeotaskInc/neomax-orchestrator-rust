use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BackupKind {
    Moved,
    Copied,
}

#[derive(Debug)]
pub(super) struct Backup {
    pub(super) path: PathBuf,
    pub(super) kind: BackupKind,
}

#[derive(Debug)]
pub(super) struct Moved {
    pub(super) target: PathBuf,
    pub(super) backup: Option<Backup>,
    pub(super) activated: bool,
}

#[derive(Debug, Default)]
pub(super) struct TransactionState {
    moved: Vec<Moved>,
}

impl TransactionState {
    pub(super) fn push(&mut self, item: Moved) {
        self.moved.push(item);
    }

    pub(super) fn mark_last_activated(&mut self) {
        self.moved
            .last_mut()
            .expect("transaction entry was just pushed")
            .activated = true;
    }

    pub(super) fn last_backup_path(&self) -> Option<&Path> {
        self.moved
            .last()
            .and_then(|item| item.backup.as_ref())
            .map(|backup| backup.path.as_path())
    }

    pub(super) fn entries(&self) -> &[Moved] {
        &self.moved
    }
}
