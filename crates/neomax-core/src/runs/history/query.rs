use std::path::PathBuf;

use rusqlite::{params, types::Type, OptionalExtension};

use crate::{Engine, Result};

use super::schema;
use super::types::{parse_status, project_from_repo, truncate, ArchivedRun, HistorySummary};
use super::HistoryStore;

const MAX_HISTORY_ROWS: usize = 10_000;

impl HistoryStore {
    pub fn list(&self, limit: usize, engine: Option<Engine>) -> Result<Vec<HistorySummary>> {
        let Some(connection) = schema::open_for_read(&self.database) else {
            return Ok(Vec::new());
        };
        let sql = if engine.is_some() {
            "SELECT id,engine,account,acct_no,status,prompt,branch,tag,goal,ultra,opus,effort,children,attempt,pr_url,started,ended,repo,project FROM runs WHERE engine=?1 ORDER BY COALESCE(ended,started) DESC LIMIT ?2"
        } else {
            "SELECT id,engine,account,acct_no,status,prompt,branch,tag,goal,ultra,opus,effort,children,attempt,pr_url,started,ended,repo,project FROM runs ORDER BY COALESCE(ended,started) DESC LIMIT ?1"
        };
        let Ok(mut statement) = connection.prepare(sql) else {
            return Ok(Vec::new());
        };
        let read = |row: &rusqlite::Row<'_>| -> rusqlite::Result<HistorySummary> {
            let engine_name: String = row.get(1)?;
            let status: String = row.get(4)?;
            let account_number = account_number(row, 3)?;
            let children = row
                .get::<_, Option<i64>>(12)?
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or_default();
            let attempt = row
                .get::<_, Option<i64>>(13)?
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(1);
            let repo = row.get::<_, Option<String>>(17)?;
            let project = row
                .get::<_, Option<String>>(18)?
                .or_else(|| project_from_repo(repo.as_deref()));
            Ok(HistorySummary {
                id: row.get(0)?,
                engine: engine_name.parse().unwrap_or(Engine::Claude),
                account: row.get(2)?,
                account_number,
                status: parse_status(&status),
                prompt: row
                    .get::<_, Option<String>>(5)?
                    .map(|value| truncate(&value, 160)),
                branch: row.get(6)?,
                tag: row
                    .get::<_, Option<String>>(7)?
                    .map(|value| truncate(&value, 120)),
                goal: row
                    .get::<_, Option<String>>(8)?
                    .map(|value| truncate(&value, 300)),
                ultra: row.get::<_, Option<bool>>(9)?.unwrap_or(false),
                opus: row.get::<_, Option<bool>>(10)?.unwrap_or(false),
                effort: row.get(11)?,
                children,
                attempt,
                pr_url: row.get(14)?,
                started: row.get(15)?,
                ended: row.get(16)?,
                repo,
                project,
            })
        };
        let limit = i64::try_from(limit.min(MAX_HISTORY_ROWS)).unwrap_or(0);
        let rows = if let Some(engine) = engine {
            match statement.query_map(params![engine.as_str(), limit], read) {
                Ok(rows) => rows,
                Err(_) => return Ok(Vec::new()),
            }
        } else {
            match statement.query_map(params![limit], read) {
                Ok(rows) => rows,
                Err(_) => return Ok(Vec::new()),
            }
        };
        Ok(rows.filter_map(std::result::Result::ok).collect())
    }

    pub fn get(&self, id: &str) -> Result<Option<ArchivedRun>> {
        let Some(connection) = schema::open_for_read(&self.database) else {
            return Ok(None);
        };
        let row = connection
            .query_row(
                "SELECT record,log_path,status FROM runs WHERE id=?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    ))
                },
            )
            .optional()
            .ok()
            .flatten();
        Ok(row.and_then(|(record, log_path, status)| {
            Some(ArchivedRun {
                run: serde_json::from_str(&record).ok()?,
                log_path: log_path.map(PathBuf::from),
                status: parse_status(&status),
            })
        }))
    }
}

fn account_number(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Option<u32>> {
    match row.get_ref(index)?.data_type() {
        Type::Null => Ok(None),
        Type::Integer => Ok(row
            .get_ref(index)?
            .as_i64()
            .ok()
            .and_then(|value| u32::try_from(value).ok())),
        Type::Real => Ok(row
            .get_ref(index)?
            .as_f64()
            .ok()
            .filter(|value| value.is_finite() && value.fract() == 0.0 && *value >= 0.0)
            .and_then(|value| u32::try_from(value as u64).ok())),
        Type::Text => {
            let value = row.get_ref(index)?.as_str().unwrap_or_default();
            if value.eq_ignore_ascii_case("orch") {
                Ok(None)
            } else {
                Ok(value.parse::<u32>().ok())
            }
        }
        Type::Blob => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::runs::{RunRecord, RunStatus};

    fn run(id: &str, status: RunStatus) -> RunRecord {
        let status = serde_json::to_value(status).unwrap();
        serde_json::from_value(serde_json::json!({
            "id":id,
            "engine":"codex",
            "model":"gpt-5.6-sol",
            "prompt":"work",
            "profile":"/profiles/.codex2",
            "workdir":"/workspace",
            "attempt":1,
            "status":status,
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
    fn archives_upserts_and_reads_the_existing_history_schema() {
        let temp = tempfile::tempdir().unwrap();
        let store = store(temp.path());
        let mut item = run("run-1", RunStatus::Error);
        store.archive(&item, Some(2), 300).unwrap();
        item.status = RunStatus::Done;
        item.extra.insert("future".into(), true.into());
        store.archive(&item, Some(2), 400).unwrap();

        let summaries = store.list(10, Some(Engine::Codex)).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].status, RunStatus::Done);
        assert_eq!(summaries[0].account_number, Some(2));
        let archived = store.get("run-1").unwrap().unwrap();
        assert_eq!(archived.run.extra.get("future").unwrap(), true);
    }

    #[test]
    fn reads_reference_text_orchestrator_account_numbers_without_failing_the_query() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("history.db");
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE runs(
                    id TEXT PRIMARY KEY, engine TEXT, account TEXT, acct_no INTEGER,
                    status TEXT, prompt TEXT, repo TEXT, branch TEXT, tag TEXT, goal TEXT,
                    effort TEXT, ultra INTEGER, opus INTEGER, model TEXT,
                    children INTEGER, attempt INTEGER, pr_url TEXT,
                    started INTEGER, ended INTEGER, archived_at INTEGER,
                    log_path TEXT, record TEXT
                );
                INSERT INTO runs(id,engine,account,acct_no,status,started,repo)
                VALUES('legacy-orch','claude','.claude-orch','orch','done',2,'service');",
            )
            .unwrap();
        let rows = store(temp.path()).list(10, None).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].account, ".claude-orch");
        assert_eq!(rows[0].account_number, None);
        assert_eq!(rows[0].project.as_deref(), Some("service"));
    }

    #[test]
    fn damaged_history_reads_as_empty() {
        let temp = tempfile::tempdir().unwrap();
        let database = temp.path().join("history.db");
        std::fs::write(&database, b"not sqlite").unwrap();
        let history = store(temp.path());
        assert!(history.list(10, None).unwrap().is_empty());
        assert!(history.get("missing").unwrap().is_none());
    }
}
