use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::Result;

use super::super::super::types::SessionTokens;
use super::super::common::{model_string, normalize_epoch};
use super::super::schema::{OpenCodeDatabase, OpenCodeMessage, OpenCodePart, OpenCodeSessionRow};
use super::connection::open_database;
use super::query::select_query;

pub fn read_messages(db: &Path, cutoff: i64) -> Result<Vec<OpenCodeMessage>> {
    let connection = open_database(db)?;
    let query = select_query(
        &connection,
        "message",
        &["id", "session_id", "time_created", "time_updated", "data"],
        Some("time_created"),
    )?;
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map([], |row| {
        Ok(OpenCodeMessage {
            id: row.get(0)?,
            session_id: row.get(1)?,
            created: normalize_epoch(row.get::<_, i64>(2)?),
            updated: row.get::<_, Option<i64>>(3)?.map(normalize_epoch),
            data: serde_json::from_str::<Value>(&row.get::<_, String>(4)?).unwrap_or(Value::Null),
        })
    })?;
    let mut messages = Vec::new();
    for row in rows {
        let message = row?;
        if cutoff == 0 || message.created >= cutoff {
            messages.push(message);
        }
    }
    Ok(messages)
}

pub fn read_sessions(db: &Path, cutoff: i64) -> Result<Vec<OpenCodeSessionRow>> {
    let connection = open_database(db)?;
    let query = select_query(
        &connection,
        "session",
        &[
            "id",
            "project_id",
            "parent_id",
            "directory",
            "title",
            "agent",
            "model",
            "time_created",
            "time_updated",
            "time_archived",
            "summary_additions",
            "summary_deletions",
            "summary_files",
            "tokens_input",
            "tokens_output",
            "tokens_reasoning",
            "tokens_cache_read",
            "tokens_cache_write",
            "cost",
        ],
        Some("time_updated"),
    )?;
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map([], |row| {
        let tokens = SessionTokens {
            input: row.get::<_, Option<i64>>(13)?.unwrap_or_default().max(0) as u64,
            output: row.get::<_, Option<i64>>(14)?.unwrap_or_default().max(0) as u64,
            reasoning: row.get::<_, Option<i64>>(15)?.unwrap_or_default().max(0) as u64,
            cache_read: row.get::<_, Option<i64>>(16)?.unwrap_or_default().max(0) as u64,
            cache_write: row.get::<_, Option<i64>>(17)?.unwrap_or_default().max(0) as u64,
            cost: row.get::<_, Option<f64>>(18)?.unwrap_or_default(),
            ..SessionTokens::default()
        };
        Ok(OpenCodeSessionRow {
            id: row.get(0)?,
            project_id: row.get(1)?,
            parent_id: row.get(2)?,
            directory: row
                .get::<_, Option<String>>(3)?
                .map(std::path::PathBuf::from),
            title: row.get(4)?,
            agent: row.get(5)?,
            model: row
                .get::<_, Option<String>>(6)?
                .and_then(|model| model_string(&Value::String(model))),
            created: row.get::<_, Option<i64>>(7)?.map(normalize_epoch),
            updated: row.get::<_, Option<i64>>(8)?.map(normalize_epoch),
            archived: row
                .get::<_, Option<i64>>(9)?
                .is_some_and(|value| value != 0),
            summary_additions: row.get::<_, Option<i64>>(10)?.unwrap_or_default().max(0) as u64,
            summary_deletions: row.get::<_, Option<i64>>(11)?.unwrap_or_default().max(0) as u64,
            summary_files: row.get::<_, Option<i64>>(12)?.unwrap_or_default().max(0) as u64,
            cost: tokens.cost,
            tokens,
            extra: BTreeMap::new(),
        })
    })?;
    let mut sessions = Vec::new();
    for row in rows {
        let session = row?;
        if cutoff == 0 || session.updated.unwrap_or_default() >= cutoff {
            sessions.push(session);
        }
    }
    Ok(sessions)
}

pub fn read_parts(db: &Path, cutoff: i64) -> Result<Vec<OpenCodePart>> {
    let connection = open_database(db)?;
    let query = select_query(
        &connection,
        "part",
        &[
            "id",
            "message_id",
            "session_id",
            "time_created",
            "time_updated",
            "data",
        ],
        Some("time_created"),
    )?;
    let mut statement = connection.prepare(&query)?;
    let rows = statement.query_map([], |row| {
        Ok(OpenCodePart {
            id: row.get(0)?,
            message_id: row.get(1)?,
            session_id: row.get(2)?,
            created: normalize_epoch(row.get::<_, i64>(3)?),
            updated: row.get::<_, Option<i64>>(4)?.map(normalize_epoch),
            data: serde_json::from_str::<Value>(&row.get::<_, String>(5)?).unwrap_or(Value::Null),
        })
    })?;
    let mut parts = Vec::new();
    for row in rows {
        let part = row?;
        if cutoff == 0 || part.created >= cutoff {
            parts.push(part);
        }
    }
    Ok(parts)
}

pub fn read_database(db: &Path, cutoff: i64) -> Result<OpenCodeDatabase> {
    Ok(OpenCodeDatabase {
        path: db.to_path_buf(),
        sessions: read_sessions(db, cutoff)?,
        messages: read_messages(db, cutoff)?,
        parts: read_parts(db, cutoff)?,
    })
}
