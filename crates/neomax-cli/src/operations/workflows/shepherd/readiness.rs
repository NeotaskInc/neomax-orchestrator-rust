use std::env;

use anyhow::{Result, bail};
use neomax_core::runs::RunStore;
use neomax_core::shepherd::{
    GitInspectionRequest, GitInspector, MergePolicy, evaluate_merge_readiness,
};

use super::super::args;
use super::decision::{decision_json, decision_text};
use crate::context::RuntimeContext;
use crate::output;

const VALUE_FLAGS: &[&str] = &["--repo", "--branch", "--base", "--expect"];
const SWITCH_FLAGS: &[&str] = &["--merge", "--json"];

pub(crate) fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(args, VALUE_FLAGS, SWITCH_FLAGS)?;
    let run = parsed
        .positionals
        .first()
        .filter(|value| !value.starts_with('-'))
        .map(|id| RunStore::new(&context.paths.runs).load(id))
        .transpose()?;
    let repository = parsed
        .value("--repo")
        .map(|value| context.resolve_path(value))
        .or_else(|| run.as_ref().and_then(|record| record.repo.clone()))
        .unwrap_or_else(|| context.cwd.clone());
    let branch = parsed
        .value("--branch")
        .map(str::to_owned)
        .or_else(|| run.as_ref().and_then(|record| record.branch.clone()));
    let base = parsed
        .value("--base")
        .map(str::to_owned)
        .or_else(|| run.as_ref().and_then(|record| record.base_ref.clone()));
    let expected = parsed.value("--expect").map(str::to_owned).or_else(|| {
        run.as_ref().and_then(|record| {
            record
                .extra
                .get("expected_sha")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        })
    });
    let inspector = GitInspector::new();
    let request = GitInspectionRequest::new(repository);
    let request = match branch {
        Some(branch) => request.branch(branch),
        None => request,
    };
    let request = match base {
        Some(base) => request.base(base),
        None => request,
    };
    let inspection = inspector.inspect(&request)?;
    let decision = evaluate_merge_readiness(
        &inspection.readiness_input(expected),
        MergePolicy::from_billing_environment(env::var("NEOMAX_CI_IGNORE_BILLING").ok().as_deref()),
    );
    if parsed.has("--merge") {
        bail!("shepherd --merge is fail-closed: Neomax never executes merge commands");
    }
    if parsed.has("--json") {
        return output::json(&decision_json(&decision));
    }
    println!("{}", decision_text(&decision));
    Ok(())
}
