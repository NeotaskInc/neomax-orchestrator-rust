use anyhow::{Result, bail};
use neomax_core::issues::{CrossRepoIssueCoordinator, CrossRepoIssueInput, LocalOnlyMirrorDriver};

use super::super::args::ParsedArgs;
use super::super::catalog::{LocalCatalog, project_name};
use super::service::issue_store;
use crate::context::RuntimeContext;
use crate::output;
use serde_json::json;

pub(super) fn run(context: &RuntimeContext, args: &ParsedArgs) -> Result<()> {
    let title = args
        .value("--title")
        .or_else(|| args.positionals.first().map(String::as_str))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("issue open requires --title or a title argument"))?;
    let project = project_name(context, args.value("--project"))?;
    let repositories = (!args.has("--all"))
        .then_some(args.value("--repos"))
        .flatten()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
    if repositories.as_ref().is_some_and(Vec::is_empty) {
        bail!("issue open --repos must contain at least one repository");
    }
    let mut input = CrossRepoIssueInput::new(
        title,
        args.value("--body").unwrap_or_default(),
        &project,
        context.now,
    );
    input.repositories = repositories;
    input.severity = args.value("--severity").map(str::to_owned);
    input.fingerprint = args.value("--fingerprint").map(str::to_owned);
    input.force_new = args.has("--force-new");
    let store = issue_store(context);
    let catalog = LocalCatalog::from_context(context);
    let driver = LocalOnlyMirrorDriver;
    let issue = CrossRepoIssueCoordinator::new(&store, &catalog, &driver).open(input)?;
    let is_new = issue.new_record;
    if args.has("--json") {
        return output::json(&json!({
            "issue": issue,
            "deduplicated": !is_new,
        }));
    }
    if is_new {
        println!("opened {}", issue.key);
    } else {
        println!("deduplicated to {}", issue.key);
    }
    Ok(())
}
