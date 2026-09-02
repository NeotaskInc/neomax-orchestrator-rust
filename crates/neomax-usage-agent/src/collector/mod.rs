mod files;
mod opencode;
mod parsers;
mod profiles;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use chrono::Utc;
use neomax_core::config::Engine;
use neomax_core::usage::UsageLedger;

use crate::config::AgentPaths;
use crate::state::WatchState;

pub(crate) use files::SourceFile;
pub(crate) use profiles::ProfileCatalog;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProviderSweep {
    pub provider: Engine,
    pub files_seen: u64,
    pub records_emitted: u64,
    pub records_skipped: u64,
    pub errors: u64,
    pub rate_limits: u64,
}

impl Default for ProviderSweep {
    fn default() -> Self {
        Self {
            provider: Engine::Claude,
            files_seen: 0,
            records_emitted: 0,
            records_skipped: 0,
            errors: 0,
            rate_limits: 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SweepReport {
    pub records_emitted: u64,
    pub files_seen: u64,
    pub files_changed: u64,
    pub records_skipped: u64,
    pub errors: u64,
    pub rate_limits: u64,
    pub providers: Vec<ProviderSweep>,
}

impl SweepReport {
    fn add_provider(&mut self, provider: ProviderSweep) {
        self.records_emitted += provider.records_emitted;
        self.records_skipped += provider.records_skipped;
        self.errors += provider.errors;
        self.rate_limits = self.rate_limits.saturating_add(provider.rate_limits);
        self.files_seen += provider.files_seen;
        if let Some(existing) = self
            .providers
            .iter_mut()
            .find(|existing| existing.provider == provider.provider)
        {
            existing.files_seen += provider.files_seen;
            existing.records_emitted += provider.records_emitted;
            existing.records_skipped += provider.records_skipped;
            existing.errors += provider.errors;
            existing.rate_limits = existing.rate_limits.saturating_add(provider.rate_limits);
        } else {
            self.providers.push(provider);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepMode {
    Incremental,
    Full,
    Baseline,
}

#[derive(Debug, Clone)]
pub struct UsageCollector {
    paths: AgentPaths,
    now: i64,
}

impl UsageCollector {
    pub fn new(paths: AgentPaths) -> Self {
        Self {
            paths,
            now: Utc::now().timestamp(),
        }
    }

    pub fn with_now(paths: AgentPaths, now: i64) -> Self {
        Self { paths, now }
    }

    pub fn state_path(&self) -> &Path {
        &self.paths.state.usage_watch
    }

    pub fn ledger(&self) -> UsageLedger {
        UsageLedger::new(&self.paths.state.usage_ledger)
    }

    pub(crate) fn source_files(&self, since: i64) -> Vec<SourceFile> {
        files::discover_sources(&ProfileCatalog::discover(&self.paths), since)
    }

    pub fn sweep(
        &self,
        state: &mut WatchState,
        mode: SweepMode,
        recent_days: u32,
    ) -> Result<SweepReport> {
        state.validate()?;
        let since = if matches!(mode, SweepMode::Full) || recent_days == 0 {
            0
        } else {
            self.now.saturating_sub(i64::from(recent_days) * 86_400)
        };
        self.paths.state.ensure_runtime_dirs()?;
        let profiles = ProfileCatalog::discover(&self.paths);
        let mut output = SweepReport::default();
        let mut records = Vec::new();
        let mut changed = BTreeMap::<Engine, u64>::new();
        for source in files::discover_sources(&profiles, since) {
            let mut provider = ProviderSweep {
                provider: source.engine,
                files_seen: 1,
                ..ProviderSweep::default()
            };
            let old_len = records.len();
            let result = files::scan_source(&source, state, mode, self.now);
            match result {
                Ok(scan) => {
                    provider.records_skipped = scan.records_skipped;
                    provider.errors = scan.errors;
                    provider.rate_limits =
                        saturating_sum(scan.records.iter().map(|record| record.rate_limits));
                    records.extend(scan.records);
                    if scan.changed {
                        *changed.entry(source.engine).or_default() += 1;
                    }
                }
                Err(error) => {
                    provider.errors = 1;
                    tracing_free_error(&error);
                }
            }
            provider.records_emitted = (records.len() - old_len) as u64;
            output.add_provider(provider);
        }
        let mut database_report = ProviderSweep {
            provider: Engine::Opencode,
            ..ProviderSweep::default()
        };
        let db_records =
            opencode::collect_databases(&self.paths.home, &profiles, state, mode, since, self.now)
                .context("collect OpenCode local telemetry")?;
        database_report.files_seen = db_records.databases_seen;
        database_report.records_skipped = db_records.records_skipped;
        database_report.errors = db_records.errors;
        database_report.rate_limits =
            saturating_sum(db_records.records.iter().map(|record| record.rate_limits));
        database_report.records_emitted = db_records.records.len() as u64;
        if database_report.files_seen > 0 {
            changed.insert(Engine::Opencode, database_report.files_seen);
        }
        records.extend(db_records.records);
        output.add_provider(database_report);
        if !records.is_empty() {
            self.ledger().append(&records)?;
        }
        output.files_changed = changed.values().sum();
        for engine in Engine::ALL {
            if !output
                .providers
                .iter()
                .any(|provider| provider.provider == engine)
            {
                output.providers.push(ProviderSweep {
                    provider: engine,
                    ..ProviderSweep::default()
                });
            }
        }
        output.providers.sort_by_key(|item| item.provider);
        Ok(output)
    }

    pub fn has_sources(&self) -> bool {
        !self.source_files(0).is_empty()
            || ProfileCatalog::discover(&self.paths)
                .for_engine(Engine::Opencode)
                .iter()
                .any(|profile| opencode::database_path(&self.paths.home, profile).is_file())
    }
}

fn saturating_sum(values: impl Iterator<Item = u64>) -> u64 {
    values.fold(0, |total, value| total.saturating_add(value))
}

fn tracing_free_error(error: &anyhow::Error) {
    let _ = error;
}

pub(crate) fn modified_epoch(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|stamp| stamp.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn account_name(profile: &Path) -> String {
    profile
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("default")
        .to_owned()
}

pub(crate) fn source_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests;
