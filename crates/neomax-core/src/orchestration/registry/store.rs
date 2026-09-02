use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::atomic::{read_json, with_exclusive_lock, write_json_atomic};
use crate::runs::{ProbeState, ProcessProbe};
use crate::{Error, Result};

use super::record::DEDICATED_ACCOUNT_MARKER;
use super::{OrchestratorRecord, OrchestratorRegistration};

const GC_AGE_SECONDS: i64 = 24 * 60 * 60;

pub struct OrchestratorStore {
    directory: PathBuf,
}

impl OrchestratorStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn register(&self, registration: OrchestratorRegistration) -> Result<OrchestratorRecord> {
        self.register_with_metadata(registration, BTreeMap::new())
    }

    /// Register an interactive orchestrator and retain launch metadata that
    /// is not part of the stable registry identity.
    ///
    /// The metadata is deliberately flattened into the record so older
    /// registries can still be read and newer launchers can preserve worker
    /// scope and project/session context across a handoff.
    pub fn register_with_metadata(
        &self,
        registration: OrchestratorRegistration,
        metadata: BTreeMap<String, Value>,
    ) -> Result<OrchestratorRecord> {
        let path = self.path_for(&registration.session);
        let lock = self.lock_for(&registration.session);
        with_exclusive_lock(&lock, || {
            let started = match read_json::<OrchestratorRecord>(&path) {
                Ok(record) => record.started,
                Err(error) if is_missing(&error) => registration.now,
                Err(error) => return Err(error),
            };
            let mut record = OrchestratorRecord::from_registration(registration, started);
            let dedicated = record.is_dedicated_account();
            record.extra = metadata;
            if dedicated {
                record.mark_dedicated_account();
            } else {
                record.extra.remove(DEDICATED_ACCOUNT_MARKER);
            }
            write_json_atomic(&path, &record)?;
            Ok(record)
        })
    }

    pub fn heartbeat(&self, session: &str, now: i64) -> Result<bool> {
        let path = self.path_for(session);
        with_exclusive_lock(&self.lock_for(session), || {
            let Ok(mut record) = read_json::<OrchestratorRecord>(&path) else {
                return Ok(false);
            };
            record.last_seen = now;
            record.live = false;
            write_json_atomic(&path, &record)?;
            Ok(true)
        })
    }

    pub fn unregister(&self, session: &str) -> Result<bool> {
        let path = self.path_for(session);
        with_exclusive_lock(&self.lock_for(session), || {
            let _path_guard = crate::io::PathGuard::for_path(&path)?;
            crate::io::reject_reparse_components(&path)?;
            match fs::remove_file(&path) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error.into()),
            }
        })
    }

    pub fn all(&self, probe: &impl ProcessProbe, now: i64) -> Result<Vec<OrchestratorRecord>> {
        let _directory_guard = crate::io::PathGuard::for_directory(&self.directory)?;
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut paths = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        paths.sort();
        let mut records = Vec::new();
        for path in paths {
            let Ok(mut record) = read_json::<OrchestratorRecord>(&path) else {
                continue;
            };
            let process_state = record
                .pid
                .map_or(ProbeState::Dead, |pid| probe.pid_state(pid));
            record.process_state = process_state;
            record.live = process_state == ProbeState::Alive;
            if process_state == ProbeState::Dead
                && now.saturating_sub(record.last_seen) > GC_AGE_SECONDS
            {
                let Ok(_path_guard) = crate::io::PathGuard::for_path(&path) else {
                    continue;
                };
                if crate::io::reject_reparse_components(&path).is_err() {
                    continue;
                }
                let _ = fs::remove_file(path);
                continue;
            }
            records.push(record);
        }
        Ok(records)
    }

    pub fn live(&self, probe: &impl ProcessProbe, now: i64) -> Result<Vec<OrchestratorRecord>> {
        Ok(self
            .all(probe, now)?
            .into_iter()
            .filter(|record| record.live)
            .collect())
    }

    pub fn on_account(
        &self,
        profile: &Path,
        engine: crate::Engine,
        exclude_session: Option<&str>,
        probe: &impl ProcessProbe,
        now: i64,
    ) -> Result<Vec<OrchestratorRecord>> {
        let account = profile
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        Ok(self
            .live(probe, now)?
            .into_iter()
            .filter(|record| {
                record.engine == engine
                    && record.account_dir == account
                    && exclude_session.is_none_or(|session| record.session != session)
            })
            .collect())
    }

    fn path_for(&self, session: &str) -> PathBuf {
        self.directory
            .join(format!("{}.json", safe_session(session)))
    }

    fn lock_for(&self, session: &str) -> PathBuf {
        self.directory
            .join(format!("{}.lock", safe_session(session)))
    }
}

fn is_missing(error: &Error) -> bool {
    matches!(error, Error::Io(error) if error.kind() == std::io::ErrorKind::NotFound)
}

fn safe_session(session: &str) -> String {
    let value = session
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() || matches!(value, '.' | '_' | '-') {
                value
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() { "_".into() } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;
    use crate::runs::ProbeState;

    struct Probe;

    impl ProcessProbe for Probe {
        fn pid_alive(&self, pid: u32) -> bool {
            pid == 42
        }

        fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
            false
        }
    }

    struct UnknownProbe;

    impl ProcessProbe for UnknownProbe {
        fn pid_alive(&self, _pid: u32) -> bool {
            false
        }

        fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
            false
        }

        fn pid_state(&self, _pid: u32) -> ProbeState {
            ProbeState::Unknown
        }
    }

    fn registration(session: &str, now: i64) -> OrchestratorRegistration {
        OrchestratorRegistration {
            session: session.into(),
            pid: Some(42),
            engine: Engine::Codex,
            account: Some(2),
            account_dir: ".codex2".into(),
            project: Some("project".into()),
            branch_prefix: Some("proj".into()),
            cwd: "/workspace".into(),
            model: "gpt-5.6-sol".into(),
            reserved: false,
            now,
        }
    }

    #[test]
    fn registration_preserves_start_and_heartbeat_refreshes_liveness() {
        let temp = tempfile::tempdir().unwrap();
        let store = OrchestratorStore::new(temp.path());
        store.register(registration("session", 10)).unwrap();
        store.register(registration("session", 20)).unwrap();
        assert!(store.heartbeat("session", 30).unwrap());
        let records = store.live(&Probe, 30).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].started, 10);
        assert_eq!(records[0].last_seen, 30);
        assert_eq!(
            store
                .on_account(
                    Path::new("/profiles/.codex2"),
                    Engine::Codex,
                    None,
                    &Probe,
                    30
                )
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn session_names_cannot_escape_the_registry_and_stale_entries_are_collected() {
        let temp = tempfile::tempdir().unwrap();
        let store = OrchestratorStore::new(temp.path());
        let mut stale = registration("../../outside", 1);
        stale.pid = Some(7);
        store.register(stale).unwrap();
        assert!(store.path_for("../../outside").starts_with(temp.path()));
        assert!(store.all(&Probe, GC_AGE_SECONDS + 2).unwrap().is_empty());
        assert!(!store.path_for("../../outside").exists());
    }

    #[test]
    fn unknown_process_liveness_does_not_collect_a_stale_registry_entry() {
        let temp = tempfile::tempdir().unwrap();
        let store = OrchestratorStore::new(temp.path());
        store.register(registration("unknown", 1)).unwrap();
        let records = store.all(&UnknownProbe, GC_AGE_SECONDS + 2).unwrap();
        assert_eq!(records.len(), 1);
        assert!(!records[0].live);
        assert!(store.path_for("unknown").exists());
    }

    #[test]
    fn reserved_registration_writes_the_legacy_dedicated_selector() {
        let temp = tempfile::tempdir().unwrap();
        let store = OrchestratorStore::new(temp.path());
        let mut value = registration("dedicated", 10);
        value.account = None;
        value.account_dir = ".claude-orch".into();
        value.reserved = true;

        let record = store.register(value).unwrap();
        assert_eq!(
            record.account_identity(),
            Some(super::super::OrchestratorAccount::Dedicated)
        );
        let raw: Value = read_json(&store.path_for("dedicated")).unwrap();
        assert_eq!(raw["account"], "orch");
        assert!(
            !raw.as_object()
                .expect("record is an object")
                .contains_key("__neomax_orchestrator_account")
        );
    }

    #[test]
    fn registration_does_not_overwrite_a_malformed_existing_record() {
        let temp = tempfile::tempdir().unwrap();
        let store = OrchestratorStore::new(temp.path());
        let path = store.path_for("malformed");
        fs::create_dir_all(temp.path()).unwrap();
        let original = br#"{"session":"malformed","account":false}"#;
        fs::write(&path, original).unwrap();

        let error = store.register(registration("malformed", 20)).unwrap_err();
        assert!(matches!(error, Error::InvalidState { .. }));
        assert_eq!(fs::read(path).unwrap(), original);
    }
}
