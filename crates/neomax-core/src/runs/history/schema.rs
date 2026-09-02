use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::Result;

pub(super) fn open(database: &Path) -> Result<Connection> {
    let _parent_guard = database
        .parent()
        .map(crate::io::PathGuard::ensure_directory)
        .transpose()?;
    #[cfg(windows)]
    crate::io::reject_reparse_components(database)?;
    let connection = Connection::open(database)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    ensure_schema(&connection)?;
    Ok(connection)
}

pub(super) fn open_for_read(database: &Path) -> Option<Connection> {
    let _path_guard = crate::io::PathGuard::for_path(database).ok()?;
    #[cfg(windows)]
    crate::io::reject_reparse_components(database).ok()?;
    let metadata = fs::symlink_metadata(database).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    let connection = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .ok()?;
    ensure_schema(&connection).ok()?;
    Some(connection)
}

fn ensure_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS runs(
            id TEXT PRIMARY KEY, engine TEXT, account TEXT, acct_no INTEGER,
            status TEXT, prompt TEXT, repo TEXT, branch TEXT, tag TEXT, goal TEXT,
            effort TEXT, ultra INTEGER, opus INTEGER, model TEXT,
            children INTEGER, attempt INTEGER, pr_url TEXT,
            started INTEGER, ended INTEGER, archived_at INTEGER,
            log_path TEXT, record TEXT, project TEXT
        );
        CREATE INDEX IF NOT EXISTS runs_started ON runs(started DESC);",
    )?;

    // Python history databases predate additive fields. ALTER only missing columns;
    // SQLite keeps every existing row and record blob intact.
    let existing = column_names(connection)?;
    for (name, declaration) in [("project", "TEXT")] {
        if !existing.contains(name) {
            let result = connection
                .execute_batch(&format!("ALTER TABLE runs ADD COLUMN {name} {declaration}"));
            if result.is_err() && !column_names(connection)?.contains(name) {
                result?;
            }
        }
    }
    Ok(())
}

fn column_names(connection: &Connection) -> rusqlite::Result<BTreeSet<String>> {
    connection
        .prepare("PRAGMA table_info(runs)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<BTreeSet<_>>>()
}
