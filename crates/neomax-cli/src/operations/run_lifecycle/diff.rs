use std::path::Path;
use std::time::Duration;

use anyhow::{Result, bail};
use neomax_core::io::{LocalProcessRunner, ProcessRequest, ProcessRunner};
use neomax_core::providers::scrub_provider_process_request;
use neomax_core::runs::{HistoryStore, RunRecord, RunStore};
use serde::Serialize;
use serde_json::Value;

use super::RunLifecycleReport;
use super::options;
use crate::context::RuntimeContext;
use crate::error;

const MAX_PATCH_BYTES: usize = 400 * 1024;
const MAX_GIT_OUTPUT: usize = 512 * 1024;

#[derive(Debug, Serialize)]
pub(crate) struct DiffReport {
    pub id: String,
    pub repo: String,
    pub branch: String,
    pub base: String,
    pub files: Vec<DiffFile>,
    pub adds: u64,
    pub dels: u64,
    pub patch: Option<String>,
    pub patch_truncated: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct DiffFile {
    pub path: String,
    pub adds: u64,
    pub dels: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SubagentDiffReport {
    pub id: String,
    pub edits: usize,
    pub files: Vec<DiffFile>,
    pub adds: u64,
    pub dels: u64,
}

pub(crate) fn run_diff(context: &RuntimeContext, args: &[String]) -> Result<RunLifecycleReport> {
    let id = error::usage(options::run_id(args))?;
    let run = load_run(context, &id)?;
    let report = diff_record(&id, &run, options::patch(args) || options::json(args))?;
    Ok(RunLifecycleReport::Diff(report))
}

pub(crate) fn run_subagent_diff(
    context: &RuntimeContext,
    args: &[String],
) -> Result<RunLifecycleReport> {
    let values = error::usage(options::positional(args, &["--json", "--patch"]))?;
    let include_patch = options::patch(args) || options::json(args);
    let id = match values.as_slice() {
        [id] => id,
        [] => bail!("subagent-diff requires an agent or session id"),
        _ => bail!("subagent-diff accepts exactly one agent or session id"),
    };
    if id.len() > 256 || id.chars().any(|value| matches!(value, '/' | '\\' | '\0')) {
        bail!("subagent id contains unsafe path characters");
    }
    for run in RunStore::new(&context.paths.runs).all()? {
        if let Some(report) = child_diff(id, &run.children, include_patch) {
            return Ok(RunLifecycleReport::SubagentDiff(report));
        }
    }
    let history = HistoryStore::new(
        &context.paths.history_db,
        &context.paths.logs,
        &context.paths.history_logs,
        &context.paths.history_pending,
    );
    if let Some(archived) = history.get(id)? {
        if let Some(report) = child_diff(id, &archived.run.children, include_patch) {
            return Ok(RunLifecycleReport::SubagentDiff(report));
        }
    }
    bail!("no recorded subagent diff for {id}")
}

fn load_run(context: &RuntimeContext, id: &str) -> Result<RunRecord> {
    let store = RunStore::new(&context.paths.runs);
    match store.load(id) {
        Ok(run) => return Ok(run),
        Err(_) if !store.path(id).exists() => {}
        Err(error) => return Err(error.into()),
    }
    let history = HistoryStore::new(
        &context.paths.history_db,
        &context.paths.logs,
        &context.paths.history_logs,
        &context.paths.history_pending,
    );
    history
        .get(id)?
        .map(|archived| archived.run)
        .ok_or_else(|| anyhow::anyhow!("unknown run {id}"))
}

fn diff_record(id: &str, run: &RunRecord, include_patch: bool) -> Result<DiffReport> {
    let repo = run
        .repo
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("run {id} has no diffable repository"))?;
    let branch = run
        .branch
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("run {id} has no diffable branch"))?;
    if !repo.is_dir() {
        bail!("run repository does not exist: {}", repo.display());
    }
    valid_ref(branch)?;
    let base = run
        .base
        .as_deref()
        .or(run.base_ref.as_deref())
        .unwrap_or("HEAD");
    valid_ref(base)?;
    let runner = LocalGitRunner::default();
    let merge_base = runner
        .capture(repo, &["merge-base", base, branch], MAX_GIT_OUTPUT)?
        .trim()
        .to_owned();
    if merge_base.is_empty() {
        bail!("could not resolve merge base for {id}");
    }
    valid_ref(&merge_base)?;
    let numstat = runner.capture(
        repo,
        &["diff", "--numstat", &merge_base, branch],
        MAX_GIT_OUTPUT,
    )?;
    let files = parse_numstat(&numstat);
    let (patch, patch_truncated) = if include_patch {
        let output =
            runner.capture_optional(repo, &["diff", &merge_base, branch], MAX_PATCH_BYTES)?;
        let truncated = output.truncated;
        let patch =
            (!output.bytes.is_empty()).then(|| String::from_utf8_lossy(&output.bytes).into_owned());
        (patch, truncated)
    } else {
        (None, false)
    };
    let adds = saturating_sum(files.iter().map(|file| file.adds));
    let dels = saturating_sum(files.iter().map(|file| file.dels));
    Ok(DiffReport {
        id: id.to_owned(),
        repo: repo
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned(),
        branch: branch.to_owned(),
        base: merge_base,
        files,
        adds,
        dels,
        patch,
        patch_truncated,
    })
}

fn parse_numstat(value: &str) -> Vec<DiffFile> {
    value
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let adds = parse_count(fields.next()?);
            let dels = parse_count(fields.next()?);
            let path = fields.next()?.to_owned();
            (!path.is_empty()).then_some(DiffFile {
                path,
                adds,
                dels,
                patch: None,
            })
        })
        .collect()
}

fn parse_count(value: &str) -> u64 {
    value.parse().unwrap_or_default()
}

fn saturating_sum(values: impl Iterator<Item = u64>) -> u64 {
    values.fold(0, |total, value| total.saturating_add(value))
}

fn valid_ref(value: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('-')
        || value.contains('\0')
        || value.contains(char::is_whitespace)
        || value.contains("..")
    {
        bail!("invalid git ref {value:?}");
    }
    Ok(())
}

fn child_diff(id: &str, children: &[Value], include_patch: bool) -> Option<SubagentDiffReport> {
    let child = children.iter().find(|child| {
        ["id", "agent", "session"]
            .iter()
            .filter_map(|key| child.get(*key).and_then(Value::as_str))
            .any(|value| value == id)
    })?;
    let files = child
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|file| {
            Some(DiffFile {
                path: file.get("path")?.as_str()?.to_owned(),
                adds: file.get("adds").and_then(Value::as_u64).unwrap_or_default(),
                dels: file.get("dels").and_then(Value::as_u64).unwrap_or_default(),
                patch: include_patch
                    .then(|| file.get("patch").and_then(Value::as_str))
                    .flatten()
                    .map(str::to_owned),
            })
        })
        .collect::<Vec<_>>();
    Some(SubagentDiffReport {
        id: id.to_owned(),
        edits: child
            .get("edits")
            .and_then(Value::as_u64)
            .or_else(|| child.get("n_edits").and_then(Value::as_u64))
            .unwrap_or(files.len() as u64) as usize,
        adds: saturating_sum(files.iter().map(|file| file.adds)),
        dels: saturating_sum(files.iter().map(|file| file.dels)),
        files,
    })
}

#[derive(Default)]
struct LocalGitRunner {
    runner: LocalProcessRunner,
}

struct Captured {
    bytes: Vec<u8>,
    truncated: bool,
}

impl LocalGitRunner {
    fn capture(&self, cwd: &Path, args: &[&str], limit: usize) -> Result<String> {
        let output = self.capture_optional(cwd, args, limit)?;
        if output.truncated {
            bail!("git output exceeded the configured limit");
        }
        if !output.bytes.is_empty() {
            return Ok(String::from_utf8_lossy(&output.bytes).into_owned());
        }
        Ok(String::new())
    }

    fn capture_optional(&self, cwd: &Path, args: &[&str], limit: usize) -> Result<Captured> {
        let request = ProcessRequest::new("git")
            .args(args.iter().copied())
            .cwd(cwd.to_path_buf())
            .timeout(Duration::from_secs(15))
            .stdout_limit(limit)
            .stderr_limit(32 * 1024);
        let request = scrub_provider_process_request(request);
        let output = self.runner.capture(&request).map_err(anyhow::Error::msg)?;
        if !output.success && !output.stdout_truncated {
            bail!(
                "git {} failed with status {}",
                args.join(" "),
                output
                    .status_code
                    .map_or_else(|| "unknown".into(), |code| code.to_string())
            );
        }
        Ok(Captured {
            bytes: output.stdout,
            truncated: output.stdout_truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_totals_saturate_at_u64_max() {
        let value = format!("{}\t{}\tfirst\n1\t1\tsecond\n", u64::MAX, u64::MAX);
        let files = parse_numstat(&value);

        assert_eq!(saturating_sum(files.iter().map(|file| file.adds)), u64::MAX);
        assert_eq!(saturating_sum(files.iter().map(|file| file.dels)), u64::MAX);
    }

    #[test]
    fn subagent_numstat_totals_saturate_at_u64_max() {
        let children = vec![serde_json::json!({
            "id": "agent",
            "files": [
                {"path": "first", "adds": u64::MAX, "dels": u64::MAX},
                {"path": "second", "adds": 1, "dels": 1}
            ]
        })];

        let report = child_diff("agent", &children, false).expect("child diff");
        assert_eq!(report.adds, u64::MAX);
        assert_eq!(report.dels, u64::MAX);
    }
}
