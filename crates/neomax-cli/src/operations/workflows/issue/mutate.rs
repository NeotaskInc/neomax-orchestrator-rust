use std::collections::BTreeMap;

use anyhow::{Result, bail};
use neomax_core::issues::{CrossRepoIssueCoordinator, IssueStatus, LocalOnlyMirrorDriver};

use super::super::args::ParsedArgs;
use super::super::catalog::LocalCatalog;
use super::service::{
    IssueProcessProbe, RuntimeClaimLiveness, active_claim_belongs_elsewhere, current_session,
    issue_store,
};
use crate::context::RuntimeContext;
use crate::output;
use serde_json::json;

pub(super) fn claim(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let key = args.positional(0, "issue claim")?;
    let liveness = RuntimeClaimLiveness(&context.liveness);
    let issue = issue_store(context)
        .claim(
            key,
            current_session(),
            Some(std::process::id()),
            context.now,
            &liveness,
            &IssueProcessProbe,
        )?
        .ok_or_else(|| {
            anyhow::anyhow!("issue {key:?} is held by another live session or is unknown")
        })?;
    if args.has("--json") {
        return output::json(&issue);
    }
    println!("claimed {}", issue.key);
    Ok(())
}

pub(super) fn release(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let key = args.positional(0, "issue release")?;
    let store = issue_store(context);
    let current = store
        .load(key)?
        .ok_or_else(|| anyhow::anyhow!("unknown issue key {key:?}"))?;
    if !args.has("--any") && active_claim_belongs_elsewhere(&current, context) {
        bail!("issue {key:?} is held by another live session; pass --any to release it");
    }
    let issue = store
        .release(key, context.now)?
        .ok_or_else(|| anyhow::anyhow!("unknown issue key {key:?}"))?;
    if args.has("--json") {
        return output::json(&issue);
    }
    println!("released {}", issue.key);
    Ok(())
}

pub(super) fn set_status(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let key = args.positional(0, "issue set")?;
    let status = args
        .value("--status")
        .ok_or_else(|| anyhow::anyhow!("issue set requires --status"))?;
    let status = IssueStatus::from(status);
    if matches!(status, IssueStatus::Unknown(_)) {
        bail!("issue set --status must be open, claimed, fixing, blocked, done, or closed");
    }
    let issue = issue_store(context)
        .set_status_at(key, status, context.now)?
        .ok_or_else(|| anyhow::anyhow!("unknown issue key {key:?}"))?;
    if args.has("--json") {
        return output::json(&issue);
    }
    println!("{} -> {}", issue.key, issue.status.as_str());
    Ok(())
}

pub(super) fn link(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let key = args.positional(0, "issue link")?;
    let store = issue_store(context);
    let mut linked = false;
    if let Some(run) = args.value("--run") {
        let issue = store
            .link_run_at(key, run, context.now)?
            .ok_or_else(|| anyhow::anyhow!("unknown issue key {key:?}"))?;
        if matches!(issue.status, IssueStatus::Open | IssueStatus::Claimed) {
            store.set_status_at(key, IssueStatus::Fixing, context.now)?;
        }
        linked = true;
    }
    if let Some(pr) = args.value("--pr") {
        let (repository, url) = pr
            .split_once('=')
            .filter(|(repository, url)| !repository.is_empty() && !url.is_empty())
            .ok_or_else(|| anyhow::anyhow!("issue link --pr must be REPOSITORY=URL"))?;
        store.link_pull_request_at(key, repository, url, context.now)?;
        linked = true;
    }
    if !linked {
        bail!("issue link requires --run or --pr");
    }
    let issue = store
        .load(key)?
        .ok_or_else(|| anyhow::anyhow!("unknown issue key {key:?}"))?;
    if args.has("--json") {
        return output::json(&issue);
    }
    println!("linked {}", issue.key);
    Ok(())
}

pub(super) fn comment(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let key = args.positional(0, "issue comment")?;
    let text = args
        .positionals
        .get(1)
        .map(String::as_str)
        .or_else(|| args.value("--body"))
        .or_else(|| args.value("--comment"))
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("issue comment requires text"))?;
    let store = issue_store(context);
    let issue = store
        .load(key)?
        .ok_or_else(|| anyhow::anyhow!("unknown issue key {key:?}"))?;
    let catalog = LocalCatalog::from_context(context);
    let driver = LocalOnlyMirrorDriver;
    let mirrors =
        CrossRepoIssueCoordinator::new(&store, &catalog, &driver).comment_all(&issue, text)?;
    store.append_event_at(
        key,
        "commented",
        BTreeMap::from([("text".into(), json!(text))]),
        context.now,
    )?;
    if args.has("--json") {
        return output::json(&json!({"key": key, "mirrors": mirrors}));
    }
    println!("commented on {mirrors} local mirror(s)");
    Ok(())
}

pub(super) fn close(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let key = args.positional(0, "issue close")?;
    let project = issue_store(context)
        .load(key)?
        .as_ref()
        .map(|issue| issue.project.clone())
        .ok_or_else(|| anyhow::anyhow!("unknown issue key {key:?}"))?;
    let store = issue_store(context);
    let catalog = LocalCatalog::from_context(context);
    let driver = LocalOnlyMirrorDriver;
    let issue = CrossRepoIssueCoordinator::new(&store, &catalog, &driver)
        .close(key, args.value("--comment"), context.now)?
        .ok_or_else(|| anyhow::anyhow!("unknown issue key {key:?}"))?;
    if args.has("--json") {
        return output::json(&issue);
    }
    println!("closed {} ({project})", issue.key);
    Ok(())
}

pub(super) fn reconcile(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let project = args.value("--project");
    let store = issue_store(context);
    let catalog = LocalCatalog::from_context(context);
    let driver = LocalOnlyMirrorDriver;
    let changed = CrossRepoIssueCoordinator::new(&store, &catalog, &driver)
        .reconcile(project, context.now)?;
    if args.has("--json") {
        return output::json(&json!({"changed": changed, "project": project}));
    }
    println!("issue reconcile: {changed} issue(s) updated");
    Ok(())
}
