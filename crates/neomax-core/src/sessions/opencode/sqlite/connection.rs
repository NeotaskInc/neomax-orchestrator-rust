use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::Result;

pub(super) fn open_database(db: &Path) -> Result<Connection> {
    if !db.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("OpenCode database does not exist: {}", db.display()),
        )
        .into());
    }
    Ok(Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?)
}
