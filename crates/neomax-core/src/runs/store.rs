use std::fs;
use std::path::PathBuf;

#[cfg(test)]
use crate::atomic::JSON_READ_MAX_BYTES;
use crate::atomic::{
    read_json, update_existing_json_locked, with_exclusive_lock, write_json_atomic,
};
use crate::{Error, Result};

use super::RunRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLoadDiagnostic {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Clone)]
pub struct RunStore {
    directory: PathBuf,
}

impl RunStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn create(&self, run: &RunRecord) -> Result<()> {
        let path = self.path(&run.id);
        with_exclusive_lock(&self.lock_path(&run.id), || {
            if path.exists() {
                return Err(Error::Conflict(format!("run {} already exists", run.id)));
            }
            write_json_atomic(&path, run)
        })
    }

    pub fn save(&self, run: &RunRecord) -> Result<()> {
        let path = self.path(&run.id);
        with_exclusive_lock(&self.lock_path(&run.id), || {
            if path.exists() {
                read_json::<RunRecord>(&path)?;
            }
            write_json_atomic(&path, run)
        })
    }

    pub fn repair(&self, run: &RunRecord) -> Result<()> {
        let path = self.path(&run.id);
        with_exclusive_lock(&self.lock_path(&run.id), || write_json_atomic(&path, run))
    }

    pub fn save_preserving_kill(&self, run: &RunRecord) -> Result<RunRecord> {
        self.save_preserving_control_markers(run)
    }

    pub fn save_preserving_control_markers(&self, run: &RunRecord) -> Result<RunRecord> {
        self.update(&run.id, |persisted| {
            if persisted.attempt > run.attempt {
                return Ok(());
            }
            let killed = persisted.killed;
            let interruption = persisted.status;
            *persisted = run.clone();
            persisted.killed |= killed;
            if interruption.is_interruption() && !persisted.status.is_interruption() {
                persisted.status = interruption;
            }
            Ok(())
        })
    }

    pub fn update<F>(&self, id: &str, update: F) -> Result<RunRecord>
    where
        F: FnOnce(&mut RunRecord) -> Result<()>,
    {
        update_existing_json_locked(&self.path(id), &self.lock_path(id), update)
    }

    pub fn load(&self, id: &str) -> Result<RunRecord> {
        read_json(&self.path(id))
    }

    pub fn load_optional(&self, id: &str) -> Result<Option<RunRecord>> {
        Ok(self.load_with_diagnostic(id)?.0)
    }

    pub fn load_with_diagnostic(
        &self,
        id: &str,
    ) -> Result<(Option<RunRecord>, Option<RunLoadDiagnostic>)> {
        let path = self.path(id);
        match read_json(&path) {
            Ok(record) => Ok((Some(record), None)),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok((None, None))
            }
            Err(Error::InvalidState { path, message }) => {
                Ok((None, Some(RunLoadDiagnostic { path, message })))
            }
            Err(Error::Message(message)) if is_bounded_read_failure(&message) => {
                Ok((None, Some(RunLoadDiagnostic { path, message })))
            }
            Err(error) => Err(error),
        }
    }

    pub fn all_with_diagnostics(&self) -> Result<(Vec<RunRecord>, Vec<RunLoadDiagnostic>)> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), Vec::new()));
            }
            Err(error) => return Err(error.into()),
        };
        let mut records = Vec::new();
        let mut diagnostics = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            match read_json(&path) {
                Ok(record) => records.push(record),
                Err(Error::InvalidState { path, message }) => {
                    diagnostics.push(RunLoadDiagnostic { path, message });
                }
                Err(Error::Message(message)) if is_bounded_read_failure(&message) => {
                    diagnostics.push(RunLoadDiagnostic { path, message });
                }
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        records.sort_by(|left: &RunRecord, right: &RunRecord| left.id.cmp(&right.id));
        diagnostics.sort_by(|left, right| left.path.cmp(&right.path));
        Ok((records, diagnostics))
    }

    pub fn all(&self) -> Result<Vec<RunRecord>> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut records = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .filter_map(|path| read_json(&path).ok())
            .collect::<Vec<_>>();
        records.sort_by(|left: &RunRecord, right: &RunRecord| left.id.cmp(&right.id));
        Ok(records)
    }

    pub fn path(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.json"))
    }

    fn lock_path(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.lock"))
    }
}

fn is_bounded_read_failure(message: &str) -> bool {
    message.contains(" exceeded its ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::RunStatus;

    fn run(id: &str) -> RunRecord {
        serde_json::from_value(serde_json::json!({
            "id":id,
            "engine":"claude",
            "model":"model",
            "prompt":"prompt",
            "profile":"/profiles/.claude1",
            "workdir":"/workspace",
            "attempt":1,
            "status":"running",
            "started":1
        }))
        .unwrap()
    }

    #[test]
    fn creates_updates_and_lists_records_without_losing_unknown_data() {
        let temp = tempfile::tempdir().unwrap();
        let store = RunStore::new(temp.path());
        let mut first = run("b");
        first.extra.insert("future".into(), true.into());
        store.create(&first).unwrap();
        store.create(&run("a")).unwrap();
        assert!(store.create(&run("a")).is_err());
        let updated = store
            .update("b", |item| {
                item.status = RunStatus::Done;
                Ok(())
            })
            .unwrap();
        assert_eq!(updated.status, RunStatus::Done);
        assert_eq!(updated.extra.get("future").unwrap(), true);
        assert_eq!(
            store
                .all()
                .unwrap()
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn skips_malformed_records_without_crashing_the_fleet() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("bad.json"), "{").unwrap();
        assert!(RunStore::new(temp.path()).all().unwrap().is_empty());
    }

    #[test]
    fn malformed_direct_optional_load_is_absent_with_an_isolated_diagnostic() {
        let temp = tempfile::tempdir().unwrap();
        let store = RunStore::new(temp.path());
        let path = temp.path().join("bad.json");
        fs::write(&path, b"{").unwrap();
        assert!(store.load_optional("bad").unwrap().is_none());
        let (record, diagnostic) = store.load_with_diagnostic("bad").unwrap();
        assert!(record.is_none());
        assert_eq!(diagnostic.unwrap().path, path);
        assert_eq!(store.all_with_diagnostics().unwrap().1.len(), 1);
    }

    #[test]
    fn oversized_direct_optional_load_is_absent_with_an_isolated_diagnostic() {
        let temp = tempfile::tempdir().unwrap();
        let store = RunStore::new(temp.path());
        let path = temp.path().join("oversized.json");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .unwrap();
        file.set_len((JSON_READ_MAX_BYTES as u64) + 1).unwrap();
        assert!(store.load_optional("oversized").unwrap().is_none());
        assert!(store.load_with_diagnostic("oversized").unwrap().1.is_some());
    }

    #[test]
    fn malformed_run_state_is_not_overwritten_without_explicit_repair() {
        let temp = tempfile::tempdir().unwrap();
        let store = RunStore::new(temp.path());
        let path = temp.path().join("bad.json");
        fs::write(&path, b"{").unwrap();
        let replacement = run("bad");
        assert!(store.save(&replacement).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{");
        store.repair(&replacement).unwrap();
        assert_eq!(store.load("bad").unwrap().id, "bad");
    }

    #[test]
    fn skips_oversized_records_without_parsing_them() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("oversized.json");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .unwrap();
        file.set_len((JSON_READ_MAX_BYTES as u64) + 1).unwrap();
        assert!(RunStore::new(temp.path()).all().unwrap().is_empty());
    }

    #[test]
    fn active_saves_cannot_clear_a_concurrent_kill() {
        let temp = tempfile::tempdir().unwrap();
        let store = RunStore::new(temp.path());
        let stale = run("run");
        store.create(&stale).unwrap();
        store
            .update("run", |persisted| {
                persisted.killed = true;
                Ok(())
            })
            .unwrap();
        let saved = store.save_preserving_kill(&stale).unwrap();
        assert!(saved.killed);
        assert!(store.load("run").unwrap().killed);
    }

    #[test]
    fn stale_attempts_cannot_overwrite_a_resumed_attempt() {
        let temp = tempfile::tempdir().unwrap();
        let store = RunStore::new(temp.path());
        let stale = run("run");
        store.create(&stale).unwrap();
        let resumed = store
            .update("run", |persisted| {
                persisted.attempt = 2;
                persisted.status = RunStatus::Done;
                Ok(())
            })
            .unwrap();

        let saved = store.save_preserving_control_markers(&stale).unwrap();

        assert_eq!(saved.attempt, resumed.attempt);
        assert_eq!(saved.status, resumed.status);
        let persisted = store.load("run").unwrap();
        assert_eq!(persisted.attempt, resumed.attempt);
        assert_eq!(persisted.status, resumed.status);
    }

    #[test]
    fn concurrent_creation_has_exactly_one_winner() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().to_path_buf();
        let winners = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let directory = &directory;
                let winners = &winners;
                scope.spawn(move || {
                    if RunStore::new(directory).create(&run("one")).is_ok() {
                        winners.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                });
            }
        });
        assert_eq!(winners.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(RunStore::new(directory).load("one").unwrap().id, "one");
    }
}
