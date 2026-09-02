use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use neomax_core::config::Engine;
use neomax_core::runtime::RuntimeEnvironment;
use neomax_core::sessions::opencode::{
    database_path as canonical_database_path, database_path_for_environment, parse_message,
    read_messages,
};
use neomax_core::usage::{LedgerKind, LedgerRecord};

use crate::collector::account_name;
use crate::collector::parsers::stable_digest;
use crate::collector::profiles::ProfileCatalog;
use crate::state::WatchState;

#[derive(Debug, Clone, Default)]
pub(crate) struct DatabaseReport {
    pub records: Vec<LedgerRecord>,
    pub databases_seen: u64,
    pub records_skipped: u64,
    pub errors: u64,
}

pub(crate) fn database_path(home: &Path, profile: &Path) -> PathBuf {
    let environment = RuntimeEnvironment::process();
    if environment.home_dir().as_deref() == Some(home) {
        return database_path_for_environment(profile, home, &environment);
    }
    canonical_database_path(profile, home)
}

pub(crate) fn collect_databases(
    home: &Path,
    profiles: &ProfileCatalog,
    state: &mut WatchState,
    mode: super::SweepMode,
    since: i64,
    now: i64,
) -> Result<DatabaseReport> {
    let mut output = DatabaseReport::default();
    for profile in profiles.for_engine(Engine::Opencode) {
        let path = database_path(home, profile);
        if !path.is_file() {
            continue;
        }
        output.databases_seen += 1;
        if matches!(mode, super::SweepMode::Baseline) {
            continue;
        }
        match collect_database(&path, profile, state, mode, since, now) {
            Ok(mut rows) => {
                output.records_skipped += rows.records_skipped;
                output.records.append(&mut rows.records);
            }
            Err(_) => output.errors += 1,
        }
    }
    Ok(output)
}

fn collect_database(
    path: &Path,
    profile: &Path,
    state: &mut WatchState,
    mode: super::SweepMode,
    since: i64,
    now: i64,
) -> Result<DatabaseReport> {
    let messages = read_messages(path, since)
        .with_context(|| format!("read OpenCode database {}", path.display()))?;
    let mut output = DatabaseReport::default();
    for message in messages {
        let Some(record) = parse_message(&message) else {
            output.records_skipped += 1;
            continue;
        };
        let fingerprint = stable_digest(&serde_json::to_string(&record)?);
        let key = format!("{}:{}", path.display(), record.id);
        if !matches!(mode, super::SweepMode::Full)
            && state.database_rows.get(&key) == Some(&fingerprint)
        {
            continue;
        }
        state.database_rows.insert(key, fingerprint);
        output.records.push(LedgerRecord {
            ts: record.timestamp.max(now.saturating_sub(86_400 * 3650)),
            engine: Engine::Opencode,
            account: account_name(profile),
            model: record.model.unwrap_or_else(|| "unknown".into()),
            id: format!("opencode:{}", record.id),
            kind: LedgerKind::Add,
            session: Some(record.session_id),
            agent: record.agent,
            input: record.tokens.input,
            output: record.tokens.output,
            reasoning: record.tokens.reasoning,
            cache_write: record.tokens.cache_write,
            cache_read: record.tokens.cache_read,
            cost: record.cost,
            requests: Some(record.requests),
            completions: Some(record.completions),
            errors: record.errors,
            rate_limits: record.rate_limits,
            extra: record.extra,
        });
    }
    Ok(output)
}
