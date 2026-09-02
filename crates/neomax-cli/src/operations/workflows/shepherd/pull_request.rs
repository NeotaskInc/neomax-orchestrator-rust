use anyhow::{Result, bail};
use neomax_core::git::pull_request::{
    GitHubPullRequestAdapter, PullRequestOutcome, PullRequestRequest,
};
use neomax_core::runs::RunStore;
use neomax_core::runs::lifecycle::request_for_run;
use serde_json::json;

use super::super::args;
use crate::context::RuntimeContext;
use crate::output;

const VALUE_FLAGS: &[&str] = &["--repo", "--branch", "--base", "--expect", "--title"];
const SWITCH_FLAGS: &[&str] = &["--merge", "--json"];

pub(crate) fn execute(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(args, VALUE_FLAGS, SWITCH_FLAGS)?;
    if parsed.has("--merge") {
        bail!("pr --merge is fail-closed: Neomax never executes merge commands");
    }
    let run_id = parsed.positionals.first().cloned();
    let run = match run_id.as_deref() {
        Some(id) => Some(
            RunStore::new(&context.paths.runs)
                .load(id)
                .map_err(|error| anyhow::anyhow!("unknown run {id}: {error}"))?,
        ),
        None => None,
    };
    let repository = parsed
        .value("--repo")
        .map(|value| context.resolve_path(value))
        .or_else(|| run.as_ref().and_then(|record| record.repo.clone()))
        .unwrap_or_else(|| context.cwd.clone());
    let branch = parsed
        .value("--branch")
        .map(str::to_owned)
        .or_else(|| run.as_ref().and_then(|record| record.branch.clone()))
        .ok_or_else(|| anyhow::anyhow!("pr requires --branch or a run id"))?;
    let expected = parsed.value("--expect").map(str::to_owned).or_else(|| {
        run.as_ref().and_then(|record| {
            record
                .extra
                .get("expected_sha")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
    });
    let base = parsed
        .value("--base")
        .map(str::to_owned)
        .or_else(|| run.as_ref().and_then(|record| record.base_ref.clone()))
        .or_else(|| run.as_ref().and_then(|record| record.base.clone()));
    let mut request = run
        .as_ref()
        .and_then(request_for_run)
        .unwrap_or_else(|| PullRequestRequest::branch(repository.clone(), branch.clone()));
    request.repository = repository;
    request.branch = branch;
    request.base = base;
    request.expected_head_sha = expected;
    if let Some(title) = parsed.value("--title") {
        request.title = Some(title.to_owned());
    }
    if request.run_id.is_none() {
        let branch_id = request.branch.replace('/', "-");
        if request.title.is_none() {
            request.title = Some(request.branch.clone());
        }
        request = request
            .run_id(format!("pr-{branch_id}"))
            .profile("-")
            .result_text("Integration PR opened by Neomax.");
    }
    let outcome = GitHubPullRequestAdapter::default().open(&request)?;
    if let (Some(run_id), Some(url)) = (run_id.as_deref(), outcome.url()) {
        persist_pr_url(&RunStore::new(&context.paths.runs), run_id, url)?;
    }
    let report = pull_request_json(&outcome);
    if parsed.has("--json") {
        return output::json(&report);
    }
    println!("{}", pull_request_text(&outcome));
    Ok(())
}

pub(crate) fn persist_pr_url(store: &RunStore, run_id: &str, url: &str) -> Result<()> {
    store.update(run_id, |run| {
        run.pr_url = Some(url.to_owned());
        Ok(())
    })?;
    Ok(())
}

fn pull_request_json(outcome: &PullRequestOutcome) -> serde_json::Value {
    match outcome {
        PullRequestOutcome::Opened(receipt) | PullRequestOutcome::Existing(receipt) => json!({
            "status": if receipt.reused { "existing" } else { "opened" },
            "url": receipt.url,
            "number": receipt.number,
            "state": receipt.state,
            "branch": receipt.branch,
            "base": receipt.base,
            "reused": receipt.reused,
        }),
        PullRequestOutcome::AlreadyMerged { branch, base } => json!({
            "status": "already-merged",
            "branch": branch,
            "base": base,
        }),
        PullRequestOutcome::Stopped {
            branch,
            expected,
            actual,
        } => json!({
            "status": "stopped",
            "branch": branch,
            "reason": format!(
                "HEAD moved ({} != expected {})",
                short_sha(actual),
                short_sha(expected)
            ),
            "expected": expected,
            "actual": actual,
        }),
    }
}

fn pull_request_text(outcome: &PullRequestOutcome) -> String {
    match outcome {
        PullRequestOutcome::Opened(receipt) => {
            format!("opened draft PR {} ({})", receipt.url, receipt.branch)
        }
        PullRequestOutcome::Existing(receipt) => {
            format!(
                "existing PR {} ({})",
                receipt.url,
                receipt.state.as_deref().unwrap_or("unknown")
            )
        }
        PullRequestOutcome::AlreadyMerged { branch, base } => {
            format!("no PR needed: {branch} is already contained in {base}")
        }
        PullRequestOutcome::Stopped {
            branch,
            expected,
            actual,
        } => format!(
            "stopped: HEAD of {branch} moved ({} != expected {})",
            short_sha(actual),
            short_sha(expected)
        ),
    }
}

fn short_sha(value: &str) -> &str {
    value.get(..8).unwrap_or(value)
}
