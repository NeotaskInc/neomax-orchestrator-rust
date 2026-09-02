#[path = "shepherd/decision.rs"]
mod decision;
#[path = "shepherd/premerge.rs"]
mod premerge;
#[path = "shepherd/pull_request.rs"]
mod pull_request;
#[path = "shepherd/readiness.rs"]
mod readiness;

use anyhow::Result;

use crate::context::RuntimeContext;

pub(super) fn premerge(context: &RuntimeContext, args: &[String]) -> Result<()> {
    premerge::run(context, args)
}

pub(super) fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    readiness::run(context, args)
}

pub(super) fn pull_request(context: &RuntimeContext, args: &[String]) -> Result<()> {
    pull_request::execute(context, args)
}

#[cfg(test)]
pub(crate) use decision::{decision_json, decision_text};
#[cfg(test)]
pub(crate) use premerge::matching_live_orchestrators;
#[cfg(test)]
pub(crate) use pull_request::persist_pr_url;
