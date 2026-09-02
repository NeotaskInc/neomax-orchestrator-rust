use std::collections::BTreeSet;

use rusqlite::Connection;

use crate::Result;

pub(super) fn select_query(
    connection: &Connection,
    table: &str,
    columns: &[&str],
    order_by: Option<&str>,
) -> Result<String> {
    let available = table_columns(connection, table)?;
    let selection = columns
        .iter()
        .map(|column| {
            if available.contains(*column) {
                (*column).to_owned()
            } else {
                format!("NULL AS {column}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ordering = order_by
        .filter(|column| available.contains(*column))
        .map(|column| format!(" ORDER BY {column}"))
        .unwrap_or_default();
    Ok(format!("SELECT {selection} FROM {table}{ordering}"))
}

fn table_columns(connection: &Connection, table: &str) -> Result<BTreeSet<String>> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    let mut columns = BTreeSet::new();
    for row in rows {
        columns.insert(row?);
    }
    Ok(columns)
}
