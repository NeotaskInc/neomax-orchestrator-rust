mod archive;
mod query;
mod schema;
mod serde_helpers;
mod types;

use std::path::PathBuf;

pub use types::{ArchiveOutcome, ArchivedRun, HistorySummary};

pub struct HistoryStore {
    pub(super) database: PathBuf,
    pub(super) live_logs: PathBuf,
    pub(super) archived_logs: PathBuf,
    pub(super) pending: PathBuf,
}

impl HistoryStore {
    pub fn new(
        database: impl Into<PathBuf>,
        live_logs: impl Into<PathBuf>,
        archived_logs: impl Into<PathBuf>,
        pending: impl Into<PathBuf>,
    ) -> Self {
        Self {
            database: database.into(),
            live_logs: live_logs.into(),
            archived_logs: archived_logs.into(),
            pending: pending.into(),
        }
    }
}
