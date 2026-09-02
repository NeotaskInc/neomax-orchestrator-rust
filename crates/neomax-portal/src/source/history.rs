use anyhow::Result;

use neomax_core::runs::{HistoryStore, HistorySummary};

use super::FilesystemPortalSource;

pub(crate) fn read_history(
    source: &FilesystemPortalSource,
    limit: usize,
) -> Result<Vec<HistorySummary>> {
    if !source.paths.history_db.is_file() {
        return Ok(Vec::new());
    }
    let store = HistoryStore::new(
        source.paths.history_db.clone(),
        source.paths.logs.clone(),
        source.paths.history_logs.clone(),
        source.paths.history_pending.clone(),
    );
    store.list(limit.clamp(1, 10_000), None).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FilesystemPortalSource;

    #[test]
    fn missing_history_database_is_empty_without_creating_state() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        assert!(read_history(&source, 60).unwrap().is_empty());
        assert!(!source.paths.history_db.exists());
    }

    #[test]
    fn damaged_history_database_is_empty_for_the_portal() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        std::fs::create_dir_all(&source.paths.state).unwrap();
        std::fs::write(&source.paths.history_db, b"not sqlite").unwrap();
        assert!(read_history(&source, 60).unwrap().is_empty());
    }
}
