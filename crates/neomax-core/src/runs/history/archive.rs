use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use rusqlite::{params, types::Value as SqlValue};
use serde::{Deserialize, Serialize};

use crate::atomic::{read_json, write_json_atomic};
use crate::runs::RunRecord;
use crate::Result;

use super::schema;
use super::types::{status_name, truncate, ArchiveOutcome};
use super::HistoryStore;

const MAX_ARCHIVED_LOG_BYTES: u64 = 16 * 1024 * 1024;

impl HistoryStore {
    pub fn archive(&self, run: &RunRecord, account_number: Option<u32>, now: i64) -> Result<()> {
        let connection = schema::open(&self.database)?;
        let log_path = self.preserve_logs(run)?;
        let record = serde_json::to_string(run)?;
        let profile_account = account_number
            .map(|value| SqlValue::Integer(i64::from(value)))
            .or_else(|| derived_account_value(run));
        connection.execute(
            "INSERT INTO runs(id,engine,account,acct_no,status,prompt,repo,branch,tag,goal,effort,ultra,opus,model,children,attempt,pr_url,started,ended,archived_at,log_path,record,project)
             VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET status=excluded.status,children=excluded.children,attempt=excluded.attempt,pr_url=excluded.pr_url,ended=excluded.ended,archived_at=excluded.archived_at,log_path=COALESCE(excluded.log_path,runs.log_path),record=excluded.record,project=COALESCE(excluded.project,runs.project)",
            params![
                &run.id,
                run.engine.as_str(),
                profile_basename(run),
                profile_account,
                status_name(run.status),
                truncate(&run.prompt, 2000),
                run.repo.as_deref().and_then(file_name),
                run.branch.as_deref(),
                run.tag.as_deref(),
                run.goal.as_deref(),
                run.effort.as_deref(),
                run.ultra,
                run.opus,
                &run.model,
                run.children.len() as i64,
                i64::from(run.attempt),
                run.pr_url.as_deref(),
                run.started,
                run.ended,
                now,
                log_path.as_deref().map(|path| path.to_string_lossy().into_owned()),
                record,
                run.project.clone(),
            ],
        )?;
        Ok(())
    }

    pub fn archive_or_spill(
        &self,
        run: &RunRecord,
        account_number: Option<u32>,
        now: i64,
    ) -> Result<ArchiveOutcome> {
        if self.archive(run, account_number, now).is_ok() {
            return Ok(ArchiveOutcome::Archived);
        }
        let pending = self.pending.join(format!("{}.json", run.id));
        write_json_atomic(
            &pending,
            &PendingArchive {
                run: run.clone(),
                account_number,
            },
        )?;
        Ok(ArchiveOutcome::Spilled { pending })
    }

    pub fn reconcile_pending(&self, now: i64) -> Result<Vec<String>> {
        let entries = match fs::read_dir(&self.pending) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut archived = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(pending) = read_pending(&path) else {
                continue;
            };
            if self
                .archive(&pending.run, pending.account_number, now)
                .is_ok()
            {
                fs::remove_file(path)?;
                archived.push(pending.run.id);
            }
        }
        Ok(archived)
    }

    fn preserve_logs(&self, run: &RunRecord) -> Result<Option<PathBuf>> {
        let Some(name) = run.log.as_deref().and_then(Path::file_name) else {
            return Ok(None);
        };
        let name = name.to_string_lossy();
        let prefix = name.split(".attempt").next().unwrap_or(&name);
        let _live_guard = match crate::io::PathGuard::for_directory(&self.live_logs) {
            Ok(guard) => guard,
            Err(_) => return Ok(None),
        };
        let entries = match fs::read_dir(&self.live_logs) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let live_metadata = fs::symlink_metadata(&self.live_logs)?;
        if is_unsafe_directory(&live_metadata)
            || crate::io::reject_reparse_components(&self.live_logs).is_err()
        {
            return Ok(None);
        }
        let _archive_guard = match crate::io::PathGuard::ensure_directory(&self.archived_logs) {
            Ok(guard) => guard,
            Err(_) => return Ok(None),
        };
        let archive_metadata = fs::symlink_metadata(&self.archived_logs)?;
        if is_unsafe_directory(&archive_metadata)
            || crate::io::reject_reparse_components(&self.archived_logs).is_err()
        {
            return Ok(None);
        }
        let mut primary = None;
        for entry in entries.flatten() {
            let source = entry.path();
            let Some(file_name) = source.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if !file_name.starts_with(prefix) {
                continue;
            }
            let source_metadata = match fs::symlink_metadata(&source) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if is_unsafe_file(&source_metadata) || source_metadata.len() > MAX_ARCHIVED_LOG_BYTES {
                continue;
            }
            let destination = self.archived_logs.join(file_name);
            let incomplete = incomplete_marker_path(&destination);
            let incomplete_state = match fs::symlink_metadata(&incomplete) {
                Ok(metadata) => {
                    if is_unsafe_file(&metadata) {
                        continue;
                    }
                    true
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(_) => continue,
            };
            let destination_state = match fs::symlink_metadata(&destination) {
                Ok(metadata) => {
                    if is_unsafe_file(&metadata) || metadata.len() > MAX_ARCHIVED_LOG_BYTES {
                        continue;
                    }
                    true
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(_) => continue,
            };
            if incomplete_state {
                if destination_state && remove_stale_file(&destination).is_err() {
                    continue;
                }
                let _ = remove_stale_file(&incomplete);
                continue;
            }
            if !destination_state
                && !copy_regular_log(&source, &destination).unwrap_or(false)
            {
                continue;
            }
            if file_name.ends_with(".jsonl") {
                primary = Some(destination);
            }
        }
        Ok(primary)
    }
}

fn copy_regular_log(source: &Path, destination: &Path) -> io::Result<bool> {
    let _source_guard = crate::io::PathGuard::for_path(source)?;
    let _destination_guard = crate::io::PathGuard::for_path(destination)?;
    let source_file = crate::io::open_regular_no_follow(source)?;
    let metadata = source_file.metadata()?;
    if is_unsafe_file(&metadata) || metadata.len() > MAX_ARCHIVED_LOG_BYTES {
        return Ok(false);
    }

    let incomplete = incomplete_marker_path(destination);
    let incomplete_file = match create_incomplete_marker(&incomplete) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };

    let mut destination_options = fs::OpenOptions::new();
    destination_options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut destination_options, libc::O_NOFOLLOW);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        crate::io::reject_reparse_components(destination)?;
        destination_options
            // Keep DELETE out of the share mode while this handle owns the
            // newly-created file. A peer therefore cannot rename the partial
            // path between a failed write and handle-bound cleanup.
            .access_mode(FILE_GENERIC_WRITE | DELETE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut destination_file = match destination_options.open(destination) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let _ = discard_destination(incomplete_file, &incomplete);
            return Ok(false);
        }
        Err(error) => {
            let _ = discard_destination(incomplete_file, &incomplete);
            return Err(error);
        }
    };
    let destination_metadata = match destination_file.metadata() {
        Ok(metadata) => metadata,
        Err(error) => {
            discard_failed_copy(destination_file, destination, incomplete_file, &incomplete);
            return Err(error);
        }
    };
    if is_unsafe_file(&destination_metadata)
        || crate::io::reject_reparse_components(destination).is_err()
    {
        discard_failed_copy(destination_file, destination, incomplete_file, &incomplete);
        return Ok(false);
    }
    let copied = io::copy(
        &mut source_file.take(MAX_ARCHIVED_LOG_BYTES.saturating_add(1)),
        &mut destination_file,
    );
    let copied = match copied {
        Ok(bytes) => bytes,
        Err(error) => {
            discard_failed_copy(destination_file, destination, incomplete_file, &incomplete);
            return Err(error);
        }
    };
    if copied > MAX_ARCHIVED_LOG_BYTES {
        discard_failed_copy(destination_file, destination, incomplete_file, &incomplete);
        return Ok(false);
    }
    if let Err(error) = destination_file.sync_all() {
        discard_failed_copy(destination_file, destination, incomplete_file, &incomplete);
        return Err(error);
    }
    let _ = discard_destination(incomplete_file, &incomplete);
    Ok(true)
}

fn incomplete_marker_path(destination: &Path) -> PathBuf {
    let mut name = destination
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_default();
    name.push(".neomax-incomplete");
    destination.with_file_name(name)
}

fn create_incomplete_marker(path: &Path) -> io::Result<fs::File> {
    let _path_guard = crate::io::PathGuard::for_path(path)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        crate::io::reject_reparse_components(path)?;
        options
            .access_mode(FILE_GENERIC_WRITE | DELETE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if is_unsafe_file(&metadata) || crate::io::reject_reparse_components(path).is_err() {
        let _ = discard_destination(file, path);
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing an unsafe incomplete marker: {}", path.display()),
        ));
    }
    if let Err(error) = file.sync_all() {
        let _ = discard_destination(file, path);
        return Err(error);
    }
    Ok(file)
}

fn remove_stale_file(path: &Path) -> io::Result<()> {
    let _path_guard = crate::io::PathGuard::for_path(path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };
        crate::io::reject_reparse_components(path)?;
        options
            .access_mode(FILE_GENERIC_READ | DELETE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if is_unsafe_file(&metadata) || crate::io::reject_reparse_components(path).is_err() {
        drop(file);
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing an unsafe incomplete marker: {}", path.display()),
        ));
    }
    discard_destination(file, path)
}

fn discard_failed_copy(
    destination_file: fs::File,
    destination: &Path,
    incomplete_file: fs::File,
    incomplete: &Path,
) {
    if discard_destination(destination_file, destination).is_ok() {
        let _ = discard_destination(incomplete_file, incomplete);
    } else {
        // Keep the marker when handle-bound destination cleanup fails. The
        // next archive pass will refuse to treat a surviving destination as
        // complete until the marker can be cleared safely.
        drop(incomplete_file);
    }
}

fn discard_destination(file: fs::File, _path: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        // The destination handle is opened with DELETE access and without
        // FILE_SHARE_DELETE. Marking this exact handle for deletion avoids a
        // path-based cleanup race with a replacement file or reparse point.
        let result = mark_for_deletion_on_close(&file);
        drop(file);
        result
    }
    #[cfg(not(windows))]
    {
        drop(file);
        match fs::remove_file(_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(windows)]
fn mark_for_deletion_on_close(file: &fs::File) -> io::Result<()> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_FLAG_DELETE,
        FILE_DISPOSITION_FLAG_ON_CLOSE, FileDispositionInfo, FileDispositionInfoEx,
        SetFileInformationByHandle,
    };

    let handle = file.as_raw_handle();
    let mut disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_ON_CLOSE,
    };
    let disposition_size = u32::try_from(size_of::<FILE_DISPOSITION_INFO_EX>())
        .expect("Windows disposition structure fits in u32");
    let extended = unsafe {
        SetFileInformationByHandle(
            handle,
            FileDispositionInfoEx,
            (&mut disposition as *mut FILE_DISPOSITION_INFO_EX).cast(),
            disposition_size,
        )
    };
    if extended != 0 {
        return Ok(());
    }

    // FileDispositionInfoEx is unavailable on older Windows/filesystems.
    // The legacy class has the same handle-bound close behavior and is
    // available on every supported Windows release.
    let mut legacy = FILE_DISPOSITION_INFO { DeleteFile: true };
    let legacy_size = u32::try_from(size_of::<FILE_DISPOSITION_INFO>())
        .expect("Windows disposition structure fits in u32");
    let legacy = unsafe {
        SetFileInformationByHandle(
            handle,
            FileDispositionInfo,
            (&mut legacy as *mut FILE_DISPOSITION_INFO).cast(),
            legacy_size,
        )
    };
    if legacy != 0 {
        return Ok(());
    }
    Err(io::Error::from_raw_os_error(unsafe {
        GetLastError() as i32
    }))
}

fn is_unsafe_directory(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || is_reparse_point(metadata)
}

fn is_unsafe_file(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
        || !metadata.is_file()
        || is_reparse_point(metadata)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingArchive {
    run: RunRecord,
    #[serde(default)]
    account_number: Option<u32>,
}

fn read_pending(path: &Path) -> Result<PendingArchive> {
    match read_json::<PendingArchive>(path) {
        Ok(pending) => Ok(pending),
        Err(_) => Ok(PendingArchive {
            run: read_json(path)?,
            account_number: None,
        }),
    }
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(str::to_string)
}

fn profile_basename(run: &RunRecord) -> String {
    run.profile
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| run.account())
}

fn derived_account_value(run: &RunRecord) -> Option<SqlValue> {
    let name = profile_basename(run);
    if run.account().eq_ignore_ascii_case("orch") {
        return Some(SqlValue::Text("orch".into()));
    }
    if name == crate::providers::catalog::spec(run.engine).default_profile_dir {
        return Some(SqlValue::Integer(1));
    }
    if name.ends_with("-acct") {
        return None;
    }
    name.rsplit_once("-acct")
        .and_then(|(_, number)| number.parse::<u32>().ok())
        .map(|value| SqlValue::Integer(i64::from(value)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::{history::types::status_name, RunStatus};

    fn run(id: &str, status: RunStatus) -> RunRecord {
        serde_json::from_value(serde_json::json!({
            "id":id,
            "engine":"codex",
            "model":"gpt-5.6-sol",
            "prompt":"work",
            "profile":"/profiles/.codex2",
            "workdir":"/workspace",
            "attempt":1,
            "status":status_name(status),
            "started":100,
            "ended":200,
            "acknowledged":false
        }))
        .unwrap()
    }

    fn store(root: &Path) -> HistoryStore {
        HistoryStore::new(
            root.join("history.db"),
            root.join("logs"),
            root.join("history-logs"),
            root.join("history-pending"),
        )
    }

    #[test]
    fn preserves_attempt_logs_and_spills_when_sqlite_is_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        fs::create_dir_all(temp.path().join("logs")).unwrap();
        fs::write(temp.path().join("logs/run-1.attempt1.jsonl"), "{}").unwrap();
        let mut item = run("run-1", RunStatus::Done);
        item.log = Some(temp.path().join("logs/run-1.attempt1.jsonl"));
        store.archive(&item, None, 300).unwrap();
        assert!(store.get("run-1").unwrap().unwrap().log_path.is_some());

        let blocked = temp.path().join("blocked");
        fs::write(&blocked, "not a directory").unwrap();
        let spill_store = HistoryStore::new(
            blocked.join("history.db"),
            temp.path().join("logs"),
            temp.path().join("history-logs"),
            temp.path().join("pending"),
        );
        let outcome = spill_store.archive_or_spill(&item, None, 300).unwrap();
        assert!(matches!(outcome, ArchiveOutcome::Spilled { .. }));
        assert!(temp.path().join("pending/run-1.json").exists());
    }

    #[test]
    fn reconcile_pending_skips_partial_and_oversized_records() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        fs::create_dir_all(temp.path().join("history-pending")).unwrap();
        fs::write(temp.path().join("history-pending/partial.json"), b"{").unwrap();
        let oversized = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(temp.path().join("history-pending/oversized.json"))
            .unwrap();
        oversized
            .set_len((crate::atomic::JSON_READ_MAX_BYTES as u64) + 1)
            .unwrap();
        assert!(store.reconcile_pending(300).unwrap().is_empty());
    }

    #[test]
    fn reconcile_pending_missing_directory_is_empty() {
        let temp = tempfile::tempdir().unwrap();
        assert!(store(temp.path())
            .reconcile_pending(300)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn reconciling_pending_runs_rederives_their_account_number() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        fs::create_dir_all(&store.pending).unwrap();
        let mut item = run("pending-acct", RunStatus::Done);
        item.profile = "/profiles/.kimi-code-acct9".into();
        write_json_atomic(&store.pending.join("pending-acct.json"), &item).unwrap();

        assert_eq!(store.reconcile_pending(300).unwrap(), ["pending-acct"]);
        let row = store.list(10, None).unwrap().pop().unwrap();
        assert_eq!(row.account, ".kimi-code-acct9");
        assert_eq!(row.account_number, Some(9));
        assert!(!store.pending.join("pending-acct.json").exists());
    }

    #[test]
    fn spilled_pending_runs_retain_an_explicit_account_number() {
        let temp = tempfile::tempdir().unwrap();
        let blocked = temp.path().join("blocked");
        fs::write(&blocked, "not a directory").unwrap();
        let store = HistoryStore::new(
            blocked.join("history.db"),
            temp.path().join("logs"),
            temp.path().join("history-logs"),
            temp.path().join("pending"),
        );
        let mut item = run("pending-explicit", RunStatus::Done);
        item.profile = "/profiles/configured-profile".into();
        assert!(matches!(
            store.archive_or_spill(&item, Some(17), 300).unwrap(),
            ArchiveOutcome::Spilled { .. }
        ));

        let recovered = HistoryStore::new(
            temp.path().join("recovered.db"),
            temp.path().join("logs"),
            temp.path().join("history-logs"),
            temp.path().join("pending"),
        );
        assert_eq!(
            recovered.reconcile_pending(301).unwrap(),
            ["pending-explicit"]
        );
        assert_eq!(
            recovered.list(10, None).unwrap()[0].account_number,
            Some(17)
        );
    }

    #[test]
    fn archives_basename_and_derives_numeric_or_orchestrator_account_markers() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        let mut numeric = run("numeric", RunStatus::Done);
        numeric.profile = "/profiles/.codex-acct2".into();
        store.archive(&numeric, None, 300).unwrap();
        let orchestrator = RunRecord::new(
            "orchestrator",
            crate::Engine::Claude,
            "claude-fable-5[1m]",
            "work",
            "/profiles/.claude-orch",
            "/workspace",
            1,
        );
        store.archive(&orchestrator, None, 301).unwrap();
        let connection = rusqlite::Connection::open(temp.path().join("history.db")).unwrap();
        let values = connection
            .prepare("SELECT account, acct_no FROM runs ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get_ref(1)?.data_type()))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(values[0].0, ".codex-acct2");
        assert_eq!(values[1].0, ".claude-orch");
        assert_eq!(values[1].1, rusqlite::types::Type::Text);
    }

    #[cfg(unix)]
    #[test]
    fn ignores_symlinked_source_logs() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        fs::create_dir_all(&store.live_logs).unwrap();
        let outside = temp.path().join("outside.jsonl");
        fs::write(&outside, b"outside").unwrap();
        let source = store.live_logs.join("run-1.attempt1.jsonl");
        symlink(&outside, &source).unwrap();
        let mut item = run("run-1", RunStatus::Done);
        item.log = Some(source);

        store.archive(&item, None, 300).unwrap();
        assert!(!store.archived_logs.join("run-1.attempt1.jsonl").exists());
        assert!(store.get("run-1").unwrap().unwrap().log_path.is_none());
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinked_archive_targets() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        fs::create_dir_all(&store.live_logs).unwrap();
        fs::create_dir_all(&store.archived_logs).unwrap();
        let source = store.live_logs.join("run-1.attempt1.jsonl");
        fs::write(&source, b"source").unwrap();
        let outside = temp.path().join("outside.jsonl");
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, store.archived_logs.join("run-1.attempt1.jsonl")).unwrap();
        let mut item = run("run-1", RunStatus::Done);
        item.log = Some(source);

        store.archive(&item, None, 300).unwrap();
        assert!(store
            .archived_logs
            .join("run-1.attempt1.jsonl")
            .is_symlink());
        assert!(store.get("run-1").unwrap().unwrap().log_path.is_none());
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
    }

    #[test]
    fn ignores_oversized_source_logs() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        fs::create_dir_all(&store.live_logs).unwrap();
        let source = store.live_logs.join("run-1.attempt1.jsonl");
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&source)
            .unwrap();
        file.set_len(MAX_ARCHIVED_LOG_BYTES + 1).unwrap();
        let mut item = run("run-1", RunStatus::Done);
        item.log = Some(source);

        store.archive(&item, None, 300).unwrap();
        assert!(!store.archived_logs.join("run-1.attempt1.jsonl").exists());
        assert!(store.get("run-1").unwrap().unwrap().log_path.is_none());
    }

    #[test]
    fn stale_incomplete_marker_is_cleared_before_a_retry() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        fs::create_dir_all(&store.live_logs).unwrap();
        fs::create_dir_all(&store.archived_logs).unwrap();
        let source = store.live_logs.join("run-1.attempt1.jsonl");
        fs::write(&source, b"source").unwrap();
        let destination = store.archived_logs.join("run-1.attempt1.jsonl");
        let marker = incomplete_marker_path(&destination);
        fs::write(&marker, b"").unwrap();
        let mut item = run("run-1", RunStatus::Done);
        item.log = Some(source);

        store.archive(&item, None, 300).unwrap();

        assert!(!marker.exists());
        assert!(!destination.exists());
        assert!(store.get("run-1").unwrap().unwrap().log_path.is_none());

        store.archive(&item, None, 301).unwrap();

        assert_eq!(fs::read(destination).unwrap(), b"source");
        assert!(!marker.exists());
        assert!(store.get("run-1").unwrap().unwrap().log_path.is_some());
    }

    #[test]
    fn incomplete_destination_is_removed_before_a_retry() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        fs::create_dir_all(&store.live_logs).unwrap();
        fs::create_dir_all(&store.archived_logs).unwrap();
        let source = store.live_logs.join("run-1.attempt1.jsonl");
        fs::write(&source, b"complete source").unwrap();
        let destination = store.archived_logs.join("run-1.attempt1.jsonl");
        let marker = incomplete_marker_path(&destination);
        fs::write(&destination, b"partial").unwrap();
        fs::write(&marker, b"").unwrap();
        let mut item = run("run-1", RunStatus::Done);
        item.log = Some(source);

        store.archive(&item, None, 300).unwrap();

        assert!(!marker.exists());
        assert!(!destination.exists());
        assert!(store.get("run-1").unwrap().unwrap().log_path.is_none());

        store.archive(&item, None, 301).unwrap();

        assert_eq!(fs::read(destination).unwrap(), b"complete source");
        assert!(!marker.exists());
        assert!(store.get("run-1").unwrap().unwrap().log_path.is_some());
    }

    #[cfg(windows)]
    #[test]
    fn discard_destination_deletes_the_partial_file_by_handle() {
        use std::io::Write;
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE, FILE_SHARE_READ,
            FILE_SHARE_WRITE,
        };

        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("run-1.attempt1.jsonl");
        let mut options = fs::OpenOptions::new();
        options
            .create_new(true)
            .write(true)
            .access_mode(FILE_GENERIC_WRITE | DELETE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let mut file = options.open(&destination).unwrap();
        file.write_all(b"partial").unwrap();

        let _ = discard_destination(file, &destination);

        assert!(!destination.exists());
    }
}
