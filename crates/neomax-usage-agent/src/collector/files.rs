use std::path::{Path, PathBuf};

use anyhow::Result;
use neomax_core::config::Engine;
use neomax_core::providers::catalog::GROK_DEFAULT_MODEL;
use neomax_core::sessions::{ArtifactKind, ArtifactSource, FsArtifactSource};
use neomax_core::usage::{LedgerRecord, parse_claude_line, parse_codex_line, parse_kimi_line};
use walkdir::WalkDir;

use crate::collector::parsers::{self, validate_numeric_usage};
use crate::collector::{account_name, modified_epoch, source_key};
use crate::io::{
    MAX_METADATA_BYTES, MAX_SOURCE_BYTES_PER_SWEEP, file_len, read_range, read_string,
};
use crate::state::WatchState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceKind {
    Transcript,
    KimiWire { session: String, agent: String },
    GrokUpdates { session: String, model: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceFile {
    pub path: PathBuf,
    pub engine: Engine,
    pub account: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ScanResult {
    pub records: Vec<LedgerRecord>,
    pub records_skipped: u64,
    pub errors: u64,
    pub changed: bool,
}

pub(crate) fn discover_sources(catalog: &super::ProfileCatalog, since: i64) -> Vec<SourceFile> {
    let mut output = Vec::new();
    for (engine, profile) in catalog::profile_engine_paths(catalog) {
        match engine {
            Engine::Claude => collect_walk(
                &mut output,
                profile,
                "projects",
                engine,
                SourceKind::Transcript,
                since,
            ),
            Engine::Codex => collect_walk(
                &mut output,
                profile,
                "sessions",
                engine,
                SourceKind::Transcript,
                since,
            ),
            Engine::Kimi => collect_kimi(&mut output, profile, since),
            Engine::Grok => collect_grok(&mut output, profile, since),
            Engine::Opencode => {}
        }
    }
    output.sort_by(|left, right| left.path.cmp(&right.path));
    output
}

mod catalog {
    pub(super) use super::super::profiles::profile_engine_paths;
}

fn collect_walk(
    output: &mut Vec<SourceFile>,
    profile: &Path,
    child: &str,
    engine: Engine,
    kind: SourceKind,
    since: i64,
) {
    let root = profile.join(child);
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().and_then(|ext| ext.to_str()) != Some("jsonl")
        {
            continue;
        }
        if since != 0 && modified_epoch(path) < since {
            continue;
        }
        output.push(SourceFile {
            path: path.to_path_buf(),
            engine,
            account: account_name(profile),
            kind: kind.clone(),
        });
    }
}

fn collect_kimi(output: &mut Vec<SourceFile>, profile: &Path, since: i64) {
    let root = profile.join("sessions");
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.file_name().and_then(|name| name.to_str()) != Some("wire.jsonl")
        {
            continue;
        }
        if since != 0 && modified_epoch(path) < since {
            continue;
        }
        let agent = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("main")
            .to_owned();
        let session_dir = path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .unwrap_or(profile);
        let session = read_session_id(session_dir).unwrap_or_else(|| {
            session_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("session")
                .to_owned()
        });
        output.push(SourceFile {
            path: path.to_path_buf(),
            engine: Engine::Kimi,
            account: account_name(profile),
            kind: SourceKind::KimiWire { session, agent },
        });
    }
}

fn collect_grok(output: &mut Vec<SourceFile>, profile: &Path, since: i64) {
    let source = FsArtifactSource::new(MAX_METADATA_BYTES);
    let Ok(index) = source.index(profile, 0) else {
        return;
    };
    for locator in index.by_kind(ArtifactKind::GrokSummary) {
        let Ok(Some(summary)) = source.read(locator) else {
            continue;
        };
        let updates = summary.path.with_file_name("updates.jsonl");
        if !updates.is_file() || (since != 0 && modified_epoch(&updates) < since) {
            continue;
        }
        let (session, model) = read_grok_metadata(&summary.path, &summary.bytes);
        output.push(SourceFile {
            path: updates,
            engine: Engine::Grok,
            account: account_name(profile),
            kind: SourceKind::GrokUpdates { session, model },
        });
    }
}

fn read_session_id(path: &Path) -> Option<String> {
    let data = read_string(&path.join("state.json"), MAX_METADATA_BYTES as u64).ok()?;
    let value: serde_json::Value = serde_json::from_str(&data).ok()?;
    value
        .get("sessionId")
        .or_else(|| value.get("id"))
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

fn read_grok_metadata(path: &Path, bytes: &[u8]) -> (String, String) {
    let fallback = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("session")
        .to_owned();
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return (fallback, GROK_DEFAULT_MODEL.into());
    };
    let id = value
        .pointer("/info/id")
        .or_else(|| value.get("id"))
        .and_then(|value| value.as_str())
        .unwrap_or(&fallback)
        .to_owned();
    let model = value
        .get("current_model_id")
        .and_then(|value| value.as_str())
        .unwrap_or(GROK_DEFAULT_MODEL)
        .to_owned();
    (id, model)
}

pub(crate) fn scan_source(
    source: &SourceFile,
    state: &mut WatchState,
    mode: super::SweepMode,
    fallback_ts: i64,
) -> Result<ScanResult> {
    let size = file_len(&source.path)?;
    let key = source_key(&source.path);
    let mut offset = state.files.get(&key).copied().unwrap_or(0);
    if size < offset {
        offset = 0;
    }
    if matches!(mode, super::SweepMode::Baseline) {
        state.files.insert(key, size);
        return Ok(ScanResult {
            changed: size != offset,
            ..ScanResult::default()
        });
    }
    if matches!(mode, super::SweepMode::Incremental) && size == offset {
        return Ok(ScanResult::default());
    }
    let chunk = read_chunk(
        &source.path,
        if matches!(mode, super::SweepMode::Full) {
            0
        } else {
            offset
        },
        size,
    )?;
    let complete_end = chunk.complete_end;
    let mut result = ScanResult {
        changed: true,
        ..ScanResult::default()
    };
    for line in chunk.text[..complete_end].split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        match parse_line(source, line, fallback_ts, state) {
            Ok(Some(record)) => result.records.push(record),
            Ok(None) => result.records_skipped += 1,
            Err(_) => result.errors += 1,
        }
    }
    let next = if !chunk.truncated && complete_end == chunk.text.len() {
        size
    } else if chunk.truncated && complete_end == 0 {
        offset.saturating_add(chunk.text.len() as u64)
    } else {
        offset + complete_end as u64
    };
    state.files.insert(key, next.min(size));
    Ok(result)
}

struct Chunk {
    text: String,
    complete_end: usize,
    truncated: bool,
}

fn read_chunk(path: &Path, offset: u64, size: u64) -> Result<Chunk> {
    let available = size.saturating_sub(offset);
    let length = available.min(MAX_SOURCE_BYTES_PER_SWEEP as u64) as usize;
    let bytes = read_range(path, offset, length)?;
    let truncated = available > length as u64;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let complete_end = if text.ends_with('\n') {
        text.len()
    } else {
        text.rfind('\n').map(|index| index + 1).unwrap_or(0)
    };
    Ok(Chunk {
        text,
        complete_end,
        truncated,
    })
}

fn parse_line(
    source: &SourceFile,
    line: &str,
    fallback_ts: i64,
    state: &mut WatchState,
) -> Result<Option<LedgerRecord>> {
    let record = match &source.kind {
        SourceKind::Transcript if source.engine == Engine::Claude => {
            if !validate_numeric_usage(line, Engine::Claude) {
                return Ok(None);
            }
            parse_claude_line(line, &source.account, fallback_ts).map(|mut record| {
                record.rate_limits = u64::from(line_is_rate_limit(line));
                record
            })
        }
        SourceKind::Transcript if source.engine == Engine::Codex => {
            if !validate_numeric_usage(line, Engine::Codex) {
                return Ok(None);
            }
            let model = parsers::codex_model_in_line(line);
            let session = source
                .path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("session");
            let state_key = format!("{}::{session}", source.path.display());
            if let Some(model) = model.as_deref() {
                state.codex_model.insert(state_key.clone(), model.into());
            }
            let model = state.codex_model.get(&state_key).map(String::as_str);
            let mut candidate =
                parse_codex_line(line, &source.account, session, model, fallback_ts);
            if let Some(candidate) = candidate.as_mut() {
                candidate.rate_limits = u64::from(line_is_rate_limit(line));
            }
            if let Some(candidate) = candidate {
                let total = candidate.total_tokens();
                if total <= state.codex_total.get(&state_key).copied().unwrap_or(0)
                    && candidate.rate_limits == 0
                {
                    return Ok(None);
                }
                state.codex_total.insert(state_key, total);
                Some(candidate)
            } else {
                None
            }
        }
        SourceKind::KimiWire { session, agent } => {
            if !validate_numeric_usage(line, Engine::Kimi) {
                return Ok(None);
            }
            parse_kimi_line(line, &source.account, session, agent, fallback_ts).map(|mut record| {
                record.rate_limits = u64::from(line_is_rate_limit(line));
                record
            })
        }
        SourceKind::GrokUpdates { session, model } => {
            neomax_core::sessions::grok::parse_usage_line(line, session, Some(model), fallback_ts)
                .map(|record| grok_ledger_record(record, &source.account))
        }
        SourceKind::Transcript => None,
    };
    Ok(record)
}

fn line_is_rate_limit(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    line.contains("rate_limit")
        || line.contains("rate limit")
        || line.contains("too many requests")
        || line.contains("429")
}

fn grok_ledger_record(
    record: neomax_core::sessions::grok::GrokUsageRecord,
    account: &str,
) -> LedgerRecord {
    LedgerRecord {
        ts: record.timestamp,
        engine: Engine::Grok,
        account: account.into(),
        model: record.model.unwrap_or_else(|| GROK_DEFAULT_MODEL.into()),
        id: format!("grok:{}:{}", record.session_id, record.id),
        kind: neomax_core::usage::LedgerKind::Add,
        session: Some(record.session_id),
        agent: None,
        input: record.tokens.input,
        output: record.tokens.output,
        reasoning: record.tokens.reasoning,
        cache_write: record.tokens.cache_write,
        cache_read: record.tokens.cache_read,
        cost: Some(record.cost),
        requests: Some(record.requests),
        completions: Some(record.completions),
        errors: record.errors,
        rate_limits: record.rate_limits,
        extra: record.extra,
    }
}
