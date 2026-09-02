use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{Local, TimeZone};

use crate::atomic::append_lines_locked;
use crate::io::{read_lines, FileSource, LocalFileSource, ReadLimits};
use crate::Result;

use super::types::{LedgerKind, LedgerRecord};

const MAX_LEDGER_FILE_BYTES: usize = 64 * 1024 * 1024;
const MAX_LEDGER_LINE_BYTES: usize = 2 * 1024 * 1024;
const LEDGER_READ_TIMEOUT: Duration = Duration::from_secs(30);

pub struct UsageLedger {
    directory: PathBuf,
}

impl UsageLedger {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn append(&self, records: &[LedgerRecord]) -> Result<()> {
        let mut by_date = BTreeMap::<String, Vec<Vec<u8>>>::new();
        for record in records {
            let date = Local
                .timestamp_opt(record.ts, 0)
                .single()
                .unwrap_or_else(Local::now)
                .format("%Y-%m-%d")
                .to_string();
            by_date
                .entry(date)
                .or_default()
                .push(serde_json::to_vec(record)?);
        }
        for (date, lines) in by_date {
            let path = self.directory.join(format!("{date}.jsonl"));
            append_lines_locked(&path, &lock_path(&path), &lines)?;
        }
        Ok(())
    }

    pub fn read_deduplicated(&self, days: u32, now: i64) -> Result<Vec<LedgerRecord>> {
        let cutoff = if days == 0 {
            0
        } else {
            now.saturating_sub(i64::from(days) * 86_400)
        };
        self.read_deduplicated_since(cutoff)
    }

    pub fn read_deduplicated_since(&self, cutoff: i64) -> Result<Vec<LedgerRecord>> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut files = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("jsonl"))
            .collect::<Vec<_>>();
        files.sort();
        let mut adds = BTreeMap::<String, LedgerRecord>::new();
        let mut totals = BTreeMap::<String, LedgerRecord>::new();
        for path in files {
            self.read_file(&path, cutoff, &mut adds, &mut totals);
        }
        Ok(adds.into_values().chain(totals.into_values()).collect())
    }

    fn read_file(
        &self,
        path: &Path,
        cutoff: i64,
        adds: &mut BTreeMap<String, LedgerRecord>,
        totals: &mut BTreeMap<String, LedgerRecord>,
    ) {
        let source = LocalFileSource;
        let Ok(metadata) = source.metadata(path) else {
            return;
        };
        if !metadata.regular || metadata.len > MAX_LEDGER_FILE_BYTES as u64 {
            return;
        }
        let Ok(reader) = source.open(path) else {
            return;
        };
        let Ok(limits) = ReadLimits::new(MAX_LEDGER_FILE_BYTES, LEDGER_READ_TIMEOUT) else {
            return;
        };
        let _ = read_lines(reader, limits, MAX_LEDGER_LINE_BYTES, |line| {
            let Ok(record) = serde_json::from_slice::<LedgerRecord>(line) else {
                return Ok(());
            };
            if cutoff != 0 && record.ts < cutoff {
                return Ok(());
            }
            if record.kind == LedgerKind::Total {
                keep_largest(totals, record, cumulative_tokens);
            } else {
                keep_largest(adds, record, |item| item.output);
            }
            Ok(())
        });
    }
}

fn keep_largest(
    records: &mut BTreeMap<String, LedgerRecord>,
    candidate: LedgerRecord,
    billed_value: impl Fn(&LedgerRecord) -> u64,
) {
    let replace = records
        .get(&candidate.id)
        .is_none_or(|current| billed_value(&candidate) > billed_value(current));
    if replace {
        records.insert(candidate.id.clone(), candidate);
    }
}

fn cumulative_tokens(record: &LedgerRecord) -> u64 {
    record
        .input
        .saturating_add(record.cache_read)
        .saturating_add(record.output)
}

fn lock_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::types::LedgerKind;
    use crate::Engine;

    fn record(id: &str, kind: LedgerKind, output: u64, ts: i64) -> LedgerRecord {
        LedgerRecord {
            ts,
            engine: Engine::Claude,
            account: "acct1".into(),
            model: "model".into(),
            id: id.into(),
            kind,
            session: None,
            agent: None,
            input: 10,
            output,
            reasoning: 0,
            cache_write: 0,
            cache_read: 2,
            cost: None,
            requests: None,
            completions: None,
            errors: 0,
            rate_limits: 0,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn partitions_by_real_date_and_keeps_the_largest_billed_value() {
        let temp = tempfile::tempdir().unwrap();
        let ledger = UsageLedger::new(temp.path());
        let now = Local::now().timestamp();
        ledger
            .append(&[
                record("add", LedgerKind::Add, 10, now),
                record("add", LedgerKind::Add, 20, now),
                record("total", LedgerKind::Total, 30, now),
                record("total", LedgerKind::Total, 40, now),
            ])
            .unwrap();
        let records = ledger.read_deduplicated(30, now).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(
            records
                .iter()
                .find(|record| record.id == "add")
                .unwrap()
                .output,
            20
        );
        assert_eq!(
            records
                .iter()
                .find(|record| record.id == "total")
                .unwrap()
                .output,
            40
        );
    }

    #[test]
    fn skips_malformed_and_out_of_window_records() {
        let temp = tempfile::tempdir().unwrap();
        let ledger = UsageLedger::new(temp.path());
        let now = Local::now().timestamp();
        ledger
            .append(&[record("old", LedgerKind::Add, 10, now - 10 * 86_400)])
            .unwrap();
        fs::write(temp.path().join("broken.jsonl"), "{\n").unwrap();
        assert!(ledger.read_deduplicated(2, now).unwrap().is_empty());
    }

    #[test]
    fn skips_a_file_that_exceeds_the_ledger_read_cap() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("oversized.jsonl");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_LEDGER_FILE_BYTES as u64 + 1)
            .unwrap();
        assert!(UsageLedger::new(temp.path())
            .read_deduplicated(30, Local::now().timestamp())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn stops_at_the_line_cap_without_reading_the_rest_of_a_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("oversized-line.jsonl");
        let mut bytes = vec![b'x'; MAX_LEDGER_LINE_BYTES + 1];
        bytes.extend_from_slice(b"\n");
        bytes.extend_from_slice(
            &serde_json::to_vec(&record(
                "after-cap",
                LedgerKind::Add,
                10,
                Local::now().timestamp(),
            ))
            .unwrap(),
        );
        bytes.push(b'\n');
        fs::write(path, bytes).unwrap();
        assert!(UsageLedger::new(temp.path())
            .read_deduplicated(30, Local::now().timestamp())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn preserves_concurrent_appends_to_the_same_partition() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().to_path_buf();
        let now = Local::now().timestamp();
        std::thread::scope(|scope| {
            for index in 0..8 {
                let directory = &directory;
                scope.spawn(move || {
                    UsageLedger::new(directory)
                        .append(&[record(
                            &format!("record-{index}"),
                            LedgerKind::Add,
                            index,
                            now,
                        )])
                        .unwrap();
                });
            }
        });
        assert_eq!(
            UsageLedger::new(directory)
                .read_deduplicated(30, now)
                .unwrap()
                .len(),
            8
        );
    }
}
