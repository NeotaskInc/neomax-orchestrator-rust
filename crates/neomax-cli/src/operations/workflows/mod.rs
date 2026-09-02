mod args;
mod catalog;
mod ci;
mod controls;
mod issue;
mod runs;
mod shepherd;

#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use neomax_core::orchestration::commands::Command;

use crate::context::RuntimeContext;

/// Dispatches repository and run-management workflows that do not start a provider.
///
/// The parent CLI owns command parsing and provider launch. This boundary keeps local
/// issue, CI, audit, and merge-readiness operations hermetic and independently testable.
pub(crate) fn execute(command: Command, args: &[String], context: &RuntimeContext) -> Result<()> {
    match command {
        Command::Pause => controls::set_paused(context, args, true),
        Command::Unpause => controls::set_paused(context, args, false),
        Command::Paused => controls::list(context, args),
        Command::Issue => issue::run(context, args),
        Command::CiSync => ci::run(context, args),
        Command::PullRequest => shepherd::pull_request(context, args),
        Command::Shepherd => shepherd::run(context, args),
        Command::Premerge => shepherd::premerge(context, args),
        Command::Reconcile => runs::reconcile(context, args),
        Command::Acknowledge => runs::acknowledge(context, args),
        Command::Audit => runs::audit(context, args),
        Command::Find => runs::find(context, args),
        Command::Clean => runs::clean(context, args),
        Command::Tidy => runs::tidy(context, args),
        _ => bail!("workflow command {command:?} is not owned by this adapter"),
    }
}
