use anyhow::Result;
use neomax_core::issues::{CrossRepoIssueCoordinator, IssueStatus, LocalOnlyMirrorDriver};
use serde_json::json;

use super::super::args::{self, ParsedArgs};
use super::super::catalog::LocalCatalog;
use super::render;
use super::service::{IssueProcessProbe, RuntimeClaimLiveness, current_session, issue_store};
use crate::context::RuntimeContext;
use crate::output;

pub(super) fn list(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let project = args.value("--project");
    let status = args.value("--status").map(IssueStatus::from);
    let issues = issue_store(context).list(project, status.as_ref())?;
    if args.has("--json") {
        return output::json(&issues);
    }
    if issues.is_empty() {
        println!("(no issues)");
        return Ok(());
    }
    for issue in issues {
        println!("{}", render::list_line(&issue));
    }
    Ok(())
}

pub(super) fn show(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let key = args.positional(0, "issue show")?;
    let issue = issue_store(context)
        .load(key)?
        .ok_or_else(|| anyhow::anyhow!("unknown issue key {key:?}"))?;
    if args.has("--json") {
        return output::json(&issue);
    }
    render::print_detail(&issue);
    Ok(())
}

pub(super) fn next(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let limit = if args.has("--all") {
        usize::MAX
    } else {
        args.value("--batch")
            .map_or(Ok(1), |value| args::positive(value, "--batch"))?
    };
    let project = args.value("--project");
    let store = issue_store(context);
    let liveness = RuntimeClaimLiveness(&context.liveness);
    let processes = IssueProcessProbe;
    let session = current_session();
    let pid = std::process::id();
    let mut claimed = Vec::new();
    for issue in store.list(project, None)? {
        if claimed.len() >= limit {
            break;
        }
        if !matches!(
            issue.status,
            IssueStatus::Open | IssueStatus::Claimed | IssueStatus::Fixing
        ) {
            continue;
        }
        if let Some(issue) = store.claim(
            &issue.key,
            session.clone(),
            Some(pid),
            context.now,
            &liveness,
            &processes,
        )? {
            claimed.push(issue);
        }
    }
    if args.has("--json") {
        let briefs = claimed
            .iter()
            .map(|issue| {
                let catalog = LocalCatalog::from_context(context);
                let driver = LocalOnlyMirrorDriver;
                let store = issue_store(context);
                let brief = catalog
                    .project(&issue.project)
                    .ok()
                    .map(|_| {
                        CrossRepoIssueCoordinator::new(&store, &catalog, &driver)
                            .issue_brief(issue)
                    });
                json!({"key": issue.key, "title": issue.title, "project": issue.project, "brief": brief})
            })
            .collect::<Vec<_>>();
        return output::json(&briefs);
    }
    if claimed.is_empty() {
        println!("(no open unclaimed issues)");
    } else {
        for issue in &claimed {
            println!("claimed {}", issue.key);
        }
    }
    Ok(())
}
